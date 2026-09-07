use crevice::std140::AsStd140;
use glam::Vec4;

use crate::{ResumeCtx, render_graph_nodes as engine_graph};
use crate::material::Material;
use harness::renderer::resources as render_res;

const SHADER_CODE: &str = include_str!("rotating.wgsl");

#[derive(Debug, Clone)]
pub struct RotatingConfig {
    pub color: Vec4,
    pub speed: f64,
}

#[derive(AsStd140)]
struct Immediates {
    color: Vec4,
    speed: f64,
}

pub struct Rotating {
    config: RotatingConfig,
}

#[expect(clippy::single_call_fn, reason = "Extracted into function to avoid long type names")]
fn register(config: RotatingConfig, render_graph: &mut super::RenderGraphWrapper<'_>) {
    use render_graph::ResourceConfig as Cfg;
    render_graph::node_helper!(into render_graph;
        using @shader_module: wgpu::ShaderModule = render_graph.create_resource("rotating::shader_module", Cfg::permanent());
        using @render_pipeline_layout: wgpu::PipelineLayout = render_graph.create_resource("rotating::render_pipeline_layout", Cfg::permanent());
        using @render_pipeline: wgpu::RenderPipeline = render_graph.create_resource("rotating::render_pipeline", Cfg::permanent());
        using @config: RotatingConfig = render_graph.create_resource("rotating::config", Cfg::permanent());

        CreateShaderModule
        (device: ref render_res::Device) -> (@shader_module) {
            OutputValue(device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Rotating shader"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SHADER_CODE)),
            }))
        };

        CreateRenderPipelineLayout
        (device: ref render_res::Device, world_bind_group_layout: ref engine_graph::WorldBindGroupLayout) -> (@render_pipeline_layout) {
            OutputValue(device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Rotating pipeline layout"),
                bind_group_layouts: &[Some(world_bind_group_layout)],
                immediate_size: 0,
            }))
        };

        CreateRenderPipeline
        (device: ref render_res::Device, shader_module: ref @shader_module, render_pipeline_layout: ref @render_pipeline_layout) -> (@render_pipeline) {
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
        (mut render_pass: engine_graph::RenderPass, config: ref @config, render_pipeline: ref @render_pipeline) -> (engine_graph::RenderPass) {
            render_pass.set_pipeline(render_pipeline);
            render_pass.set_immediates(0, Immediates {
                color: config.color,
                speed: config.speed,
            }.as_std140().as_bytes());
            render_pass.draw(0..3, 0..1);

            OutputValue(render_pass)
        };
    );

    render_graph.set_resource_input(config_resource, config);
}

impl Rotating {
    pub fn new(_ctx: &mut ResumeCtx<'_>, config: RotatingConfig) -> Self {
        Self { config }
    }
}

impl Material for Rotating {
    fn register(&mut self, render_graph: &mut super::RenderGraphWrapper<'_>) {
        register(self.config.clone(), render_graph);
    }
}
