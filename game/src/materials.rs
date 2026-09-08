use std::{num::NonZeroU64, rc::Rc};
use crevice::std140::AsStd140;
use engine::{ wgpu, material::Material, render_graph_nodes as engine_graph, renderer::resources as render_res };
use glam::Vec3;
use render_graph::ResourceHandle;

use crate::utils::CardinalDirection;

static SHADER_CODE: &str = include_str!("chunk_material.wgsl");

#[derive(AsStd140)]
struct Immediates {
    position: Vec3,
    direction: u32,
}

#[derive(Debug, Clone)]
pub struct ChunkRenderData {
    pub direction: CardinalDirection,
    pub position: Vec3,
    pub vertex_buffer: wgpu::Buffer,
}

struct ChunkList {
    chunks: Rc<[ChunkRenderData]>,
}

fn register(texture: wgpu::Texture, render_graph: &mut engine::material::RenderGraphWrapper<'_>) -> ResourceHandle<ChunkList> {
    use render_graph::ResourceConfig as Cfg;
    render_graph::node_helper!(into render_graph;
        using @chunk_list: ChunkList = render_graph.create_resource("chunk_material::chunk_list", Cfg::permanent());

        using @shader_module: wgpu::ShaderModule = render_graph.create_resource("chunk_material::shader_module", Cfg::permanent());
        using @texture: wgpu::Texture = render_graph.create_resource("chunk_material::texture", Cfg::permanent());

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

        CreateBindGroupLayout
        (device: ref render_res::Device) -> (@bind_group_layout) {
            OutputValue(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Chunk material"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            }))
        };

        CreateBindGroup (
            device: ref render_res::Device,
            bind_group_layout: ref @bind_group_layout,
            texture: ref @texture,
        ) -> (@bind_group) {
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: None,
                ..Default::default()
            });
            OutputValue(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Chunk material"),
                layout: bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&texture.create_view(&wgpu::TextureViewDescriptor::default()))
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
                immediate_size: Immediates::std140_size_static().try_into().unwrap(),
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
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: 4,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &[
                                wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint16, offset: 0, shader_location: 0 },
                            ],
                        })
                    ],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
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

        Draw (
            mut render_pass: engine_graph::RenderPass,
            bind_group: ref @bind_group,
            render_pipeline: ref @render_pipeline,
            chunk_list: ref @chunk_list,
        ) -> (engine_graph::RenderPass) {
            render_pass.set_pipeline(render_pipeline);
            render_pass.set_bind_group(1, bind_group, &[]);
            for chunk in &*chunk_list.chunks {
                render_pass.set_immediates(0, Immediates {
                    position: chunk.position,
                    direction: chunk.direction as u32,
                }.as_std140().as_bytes());
                render_pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
                render_pass.draw(0..4, 0..(chunk.vertex_buffer.size() / 4).try_into().unwrap());
            }

            OutputValue(render_pass)
        };
    );

    render_graph.set_resource_input(texture_resource, texture);
    render_graph.set_resource_input(chunk_list_resource, ChunkList { chunks: Rc::default() });

    chunk_list_resource
}

pub struct ChunkMaterial {
    pub texture: wgpu::Texture,
    pub chunk_list: Rc<[ChunkRenderData]>,
    chunk_list_resource: Option<ResourceHandle<ChunkList>>,
}

impl ChunkMaterial {
    pub fn new(texture: wgpu::Texture) -> Self {
        Self {
            texture,
            chunk_list: Rc::default(),
            chunk_list_resource: None,
        }
    }
}

impl Material for ChunkMaterial {
    fn register(&mut self, render_graph: &mut engine::material::RenderGraphWrapper<'_>) {
        self.chunk_list_resource = Some(register(self.texture.clone(), render_graph));
    }

    fn update(&mut self, render_graph: &mut render_graph::RenderGraph) {
        render_graph.set_resource_input(self.chunk_list_resource.unwrap(), ChunkList { chunks: Rc::clone(&self.chunk_list) });
    }
}
