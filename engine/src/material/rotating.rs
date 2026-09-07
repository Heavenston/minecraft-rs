use glam::Vec4;

use crate::{ResumeCtx, render_graph_nodes as engine_graph};
use crate::material::Material;
use harness::renderer::resources as render_res;

const SHADER_CODE: &str = include_str!("rotating.wgsl");

render_graph::graph_resource!(struct ShaderModule(wgpu::ShaderModule); permanent);
render_graph::graph_resource!(struct RenderPipelineLayout(wgpu::PipelineLayout); permanent);
render_graph::graph_resource!(struct RenderPipeline(wgpu::RenderPipeline); permanent);

pub struct RotatingConfig {
    pub color: Vec4,
    pub speed: f64,
}

pub struct Rotating;

impl Rotating {
    pub fn new(_ctx: &mut ResumeCtx<'_>, _data: RotatingConfig) -> Self {
        Self { }
    }
}

impl Material for Rotating {
    fn register(&mut self, render_graph: &mut super::RenderGraphWrapper<'_>) {
        render_graph::node_helper!(into render_graph;
            CreateShaderModule
            (device: ref render_res::Device) -> (ShaderModule) {
                OutputValue(device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("Rotating shader"),
                    source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SHADER_CODE)),
                }))
            };

            CreateRenderPipelineLayout
            (device: ref render_res::Device, world_bind_group_layout: ref engine_graph::WorldBindGroupLayout) -> (RenderPipelineLayout) {
                OutputValue(device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Rotating pipeline layout"),
                    bind_group_layouts: &[Some(world_bind_group_layout)],
                    immediate_size: 0,
                }))
            };

            CreateRenderPipeline
            (device: ref render_res::Device, shader_module: ref ShaderModule, render_pipeline_layout: ref RenderPipelineLayout) -> (RenderPipeline) {
                OutputValue(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Rotating pipeline"),
                    layout: Some(render_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: shader_module,
                        entry_point: None,
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[],
                    },
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: shader_module,
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
                }))
            };

            Draw
            (mut render_pass: engine_graph::RenderPass, render_pipeline: ref RenderPipeline,) -> (engine_graph::RenderPass) {
                render_pass.set_pipeline(render_pipeline);
                render_pass.draw(0..3, 0..1);
                OutputValue(render_pass)
            };
        );
    }
}
