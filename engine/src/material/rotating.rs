use std::num::NonZeroU64;

use crevice::std140::AsStd140;
use glam::Vec4;

use crate::{ResumeCtx, render_graph_nodes as engine_graph};
use crate::material::Material;
use harness::renderer::resources as render_res;

const SHADER_CODE: &str = include_str!("rotating.wgsl");

#[derive(Debug, Clone)]
pub struct RotatingConfig {
    pub color: Vec4,
    pub speed: f32,
}

#[derive(AsStd140)]
struct Uniform {
    color: Vec4,
    speed: f32,
}

#[expect(clippy::single_call_fn, reason = "Extracted into function to avoid long type names")]
fn register(config: RotatingConfig, render_graph: &mut super::RenderGraphWrapper<'_>) -> render_graph::ResourceHandle<RotatingConfig> {
    use render_graph::ResourceConfig as Cfg;
    render_graph::node_helper!(into render_graph;
        using @shader_module: wgpu::ShaderModule = render_graph.create_resource("rotating::shader_module", Cfg::permanent());

        using @uniform_buffer: wgpu::Buffer = render_graph.create_resource("rotating::uniform_buffer", Cfg::permanent());
        using @bind_group_layout: wgpu::BindGroupLayout = render_graph.create_resource("rotating::bind_group_layout", Cfg::permanent());
        using @bind_group: wgpu::BindGroup = render_graph.create_resource("rotating::bind_group", Cfg::permanent());

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

        CreateUniformBuffer
        (device: ref render_res::Device) -> (@uniform_buffer) {
            OutputValue(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Rotating buffer"),
                size: Uniform::std140_size_static().try_into().unwrap(),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: false,
            }))
        };

        CreateBindGroupLayout
        (device: ref render_res::Device) -> (@bind_group_layout) {
            OutputValue(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Rotating bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(Uniform::std140_size_static().try_into().unwrap()).unwrap()),
                    },
                    count: None,
                }],
            }))
        };

        CreateBindGroup (
            device: ref render_res::Device,
            uniform_buffer: ref @uniform_buffer,
            bind_group_layout: ref @bind_group_layout,
        ) -> (@bind_group) {
            OutputValue(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Rotating bind group"),
                layout: bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: uniform_buffer,
                            offset: 0,
                            size: None,
                        })
                    },
                ],
            }))
        };

        CreateRenderPipelineLayout (
            device: ref render_res::Device,
            world_bind_group_layout: ref engine_graph::WorldBindGroupLayout,
            bind_group_layout: ref @bind_group_layout,
        ) -> (@render_pipeline_layout) {
            OutputValue(device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Rotating pipeline layout"),
                bind_group_layouts: &[Some(world_bind_group_layout), Some(bind_group_layout)],
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

        WriteUniformBuffer(
            mut command_encoder: render_res::FrameCommandEncoder,
            config: ref @config,
            uniform_buffer: ref @uniform_buffer,
            staging_belt: ref engine_graph::StagingBelt,
            _: ref engine_graph::UsingStagingBelt,
        ) -> (render_res::FrameCommandEncoder) {
            let data = Uniform {
                color: config.color,
                speed: config.speed,
            }.as_std140();
            let bytes = data.as_bytes();

            staging_belt.write().write_buffer(&mut command_encoder, uniform_buffer, 0, NonZeroU64::new(bytes.len().try_into().unwrap()).unwrap())
                .copy_from_slice(bytes);
    
            OutputValue(command_encoder)
        };

        Draw (
            mut render_pass: engine_graph::RenderPass,
            bind_group: ref @bind_group,
            render_pipeline: ref @render_pipeline,
        ) -> (engine_graph::RenderPass) {
            render_pass.set_pipeline(render_pipeline);
            render_pass.set_bind_group(1, bind_group, &[]);
            render_pass.draw(0..3, 0..1);

            OutputValue(render_pass)
        };
    );

    render_graph.set_resource_input(config_resource, config);

    config_resource
}

pub struct Rotating {
    config_changed: bool,
    config: RotatingConfig,
    config_resource: Option<render_graph::ResourceHandle<RotatingConfig>>,
}

impl Rotating {
    pub fn new(_ctx: &mut ResumeCtx<'_>, config: RotatingConfig) -> Self {
        Self {
            config,
            config_changed: false,
            config_resource: None,
        }
    }

    pub fn set_config(&mut self, new_config: RotatingConfig) {
        self.config = new_config;
        self.config_changed = true;
    }
}

impl Material for Rotating {
    fn register(&mut self, render_graph: &mut super::RenderGraphWrapper<'_>) {
        self.config_resource = Some(register(self.config.clone(), render_graph));
    }

    fn update(&mut self, render_graph: &mut render_graph::RenderGraph) {
        if self.config_changed && let Some(resource) = self.config_resource {
            render_graph.set_resource_input(resource, self.config.clone());
        }
    }
}
