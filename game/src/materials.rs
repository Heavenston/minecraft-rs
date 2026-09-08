use std::num::NonZeroU64;
use crevice::std140::AsStd140;
use engine::{ wgpu, material::Material, render_graph_nodes as engine_graph, renderer::resources as render_res };

static SHADER_CODE: &str = include_str!("chunk_material.wgsl");

#[derive(AsStd140)]
struct Uniform {
    
}

fn register(render_graph: &mut engine::material::RenderGraphWrapper<'_>) {
    use render_graph::ResourceConfig as Cfg;
    render_graph::node_helper!(into render_graph;
        using @shader_module: wgpu::ShaderModule = render_graph.create_resource("chunk_material::shader_module", Cfg::permanent());

        using @uniform_buffer: wgpu::Buffer = render_graph.create_resource("chunk_material::uniform_buffer", Cfg::permanent());
        using @bind_group_layout: wgpu::BindGroupLayout = render_graph.create_resource("chunk_material::bind_group_layout", Cfg::permanent());
        using @bind_group: wgpu::BindGroup = render_graph.create_resource("chunk_material::bind_group", Cfg::permanent());

        using @render_pipeline_layout: wgpu::PipelineLayout = render_graph.create_resource("chunk_material::render_pipeline_layout", Cfg::permanent());
        using @render_pipeline: wgpu::RenderPipeline = render_graph.create_resource("chunk_material::render_pipeline", Cfg::permanent());

        CreateShaderModule
        (device: ref render_res::Device) -> (@shader_module) {
            OutputValue(device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Chunk material"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SHADER_CODE)),
            }))
        };

        CreateUniformBuffer
        (device: ref render_res::Device) -> (@uniform_buffer) {
            OutputValue(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Chunk material"),
                size: Uniform::std140_size_static().try_into().unwrap(),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: false,
            }))
        };

        CreateBindGroupLayout
        (device: ref render_res::Device) -> (@bind_group_layout) {
            OutputValue(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Chunk material"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: Some(NonZeroU64::new(Uniform::std140_size_static().try_into().unwrap()).unwrap()),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: Some(NonZeroU64::new(4).unwrap()),
                        },
                        count: None,
                    },
                ],
            }))
        };

        CreateBindGroup (
            device: ref render_res::Device,
            uniform_buffer: ref @uniform_buffer,
            bind_group_layout: ref @bind_group_layout,
        ) -> (@bind_group) {
            OutputValue(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Chunk material"),
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
                label: Some("Chunk material"),
                bind_group_layouts: &[Some(world_bind_group_layout), Some(bind_group_layout)],
                immediate_size: 0,
            }))
        };

        CreateRenderPipeline
        (device: ref render_res::Device, shader_module: ref @shader_module, render_pipeline_layout: ref @render_pipeline_layout) -> (@render_pipeline) {
            OutputValue(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Chunk material"),
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

        // WriteUniformBuffer(
        //     mut command_encoder: render_res::FrameCommandEncoder,
        //     config: ref @config,
        //     uniform_buffer: ref @uniform_buffer,
        //     staging_belt: ref engine_graph::StagingBelt,
        //     _: ref engine_graph::UsingStagingBelt,
        // ) -> (render_res::FrameCommandEncoder) {
        //     let data = Uniform {
        //         color: config.color,
        //         speed: config.speed,
        //     }.as_std140();
        //     let bytes = data.as_bytes();

        //     staging_belt.write().write_buffer(&mut command_encoder, uniform_buffer, 0, NonZeroU64::new(bytes.len().try_into().unwrap()).unwrap())
        //         .copy_from_slice(bytes);
    
        //     OutputValue(command_encoder)
        // };

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
}

pub struct ChunkMaterial;
impl Material for ChunkMaterial {
    fn register(&mut self, render_graph: &mut engine::material::RenderGraphWrapper<'_>) {
        todo!()
    }
}
