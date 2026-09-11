use std::rc::Rc;
use crevice::std140::AsStd140;
use engine::{ wgpu, material::Material, render_graph_nodes as engine_graph, renderer::resources as render_res };
use glam::Vec3;
use render_graph::ResourceHandle;

use crate::utils::CardinalDirection;

static SHADER_CODE: &str = include_str!("chunk_material.wgsl");

render_graph::graph_resource!(pub struct EnableWireframes(pub bool); permanent);

render_graph::graph_resource!(pub struct ShaderModule(pub wgpu::ShaderModule); permanent);
render_graph::graph_resource!(pub struct BindGroupLayout(pub wgpu::BindGroupLayout); permanent);
render_graph::graph_resource!(pub struct RenderPipelineLayout(pub wgpu::PipelineLayout); permanent);

render_graph::graph_resource!(pub struct OpaqueRenderStep(pub ()));
render_graph::graph_resource!(pub struct CutoutRenderStep(pub ()));
render_graph::graph_resource!(pub struct TranslucentRenderStep(pub ()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChunkTransparencyMode {
    Opaque,
    Cutout,
    Translucent,
}

pub struct ChunkRenderConfig {
    pub texture: wgpu::Texture,
    pub transparency: ChunkTransparencyMode,
}

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

fn register_global(render_graph: &mut engine::material::RenderGraphWrapper<'_>) {
    render_graph::node_helper!(into render_graph;
        CreateShaderModule
        (device: ref render_res::Device) -> (ShaderModule) {
            OutputValue(device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Chunk material"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SHADER_CODE)),
            }))
        };

        CreateBindGroupLayout
        (device: ref render_res::Device) -> (BindGroupLayout) {
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

        CreateRenderPipelineLayout (
            device: ref render_res::Device,
            world_bind_group_layout: ref engine_graph::WorldBindGroupLayout,
            bind_group_layout: ref BindGroupLayout,
        ) -> (RenderPipelineLayout) {
            OutputValue(device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Chunk material"),
                bind_group_layouts: &[Some(world_bind_group_layout), Some(bind_group_layout)],
                immediate_size: Immediates::std140_size_static().try_into().unwrap(),
            }))
        };

        BeginOpaqueStep() -> (default OpaqueRenderStep);
        BeginCutoutStep(_: OpaqueRenderStep) -> (default CutoutRenderStep);
        BeginTranslucentStep(_: CutoutRenderStep) -> (default TranslucentRenderStep);
    );
}

fn register(cfg: &ChunkRenderConfig, render_graph: &mut engine::material::RenderGraphWrapper<'_>) -> ResourceHandle<ChunkList> {
    use render_graph::ResourceConfig as Cfg;
    render_graph::node_helper!(into render_graph;
        using @chunk_list: ChunkList = render_graph.create_resource("chunk_material::chunk_list", Cfg::new());

        using @transparency: ChunkTransparencyMode = render_graph.create_resource("chunk_material::transparency", Cfg::permanent());
        using @texture: wgpu::Texture = render_graph.create_resource("chunk_material::texture", Cfg::permanent());

        using @bind_group: wgpu::BindGroup = render_graph.create_resource("chunk_material::bind_group", Cfg::permanent());
        using @render_pipeline: wgpu::RenderPipeline = render_graph.create_resource("chunk_material::render_pipeline", Cfg::permanent());

        using @draw_step: () = match cfg.transparency {
            ChunkTransparencyMode::Opaque => render_graph.resource_from_type::<OpaqueRenderStep>(),
            ChunkTransparencyMode::Cutout => render_graph.resource_from_type::<CutoutRenderStep>(),
            ChunkTransparencyMode::Translucent => render_graph.resource_from_type::<TranslucentRenderStep>(),
        };

        CreateBindGroup (
            device: ref render_res::Device,
            bind_group_layout: ref BindGroupLayout,
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

        CreateRenderPipeline(
            device: ref render_res::Device,
            shader_module: ref ShaderModule, render_pipeline_layout: ref RenderPipelineLayout,
            &enable_wireframes: ref EnableWireframes,
            &transparency: ref @transparency,
        ) -> (@render_pipeline) {
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
                                wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 0, shader_location: 0 },
                            ],
                        })
                    ],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    polygon_mode: if enable_wireframes {
                        if device.features().contains(wgpu::Features::POLYGON_MODE_LINE) {
                            wgpu::PolygonMode::Line
                        }
                        else {
                            tracing::warn!("Device does not support POLYGON_MODE_LINE");
                            wgpu::PolygonMode::Fill
                        }
                    } else {
                        wgpu::PolygonMode::Fill
                    },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(match transparency {
                        ChunkTransparencyMode::Opaque | ChunkTransparencyMode::Cutout => true,
                        ChunkTransparencyMode::Translucent => false,
                    }),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: shader_module,
                    entry_point: None,
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[
                            ("ENABLE_CUTOUT", match transparency {
                                ChunkTransparencyMode::Cutout => 1.,
                                ChunkTransparencyMode::Opaque | ChunkTransparencyMode::Translucent => 0.,
                            }),
                        ],
                        ..wgpu::PipelineCompilationOptions::default()
                    },
                    targets: &[
                        Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Bgra8UnormSrgb,
                            blend: match transparency {
                                ChunkTransparencyMode::Opaque | ChunkTransparencyMode::Cutout => None,
                                ChunkTransparencyMode::Translucent => Some(wgpu::BlendState::ALPHA_BLENDING),
                            },
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
            chunk_list: @chunk_list,

            &transparency: ref @transparency,
            _: ref @draw_step,
        ) -> (engine_graph::RenderPass) {
            render_pass.push_debug_group(&format!("{transparency:?} Chunk renderer"));
            render_pass.set_pipeline(render_pipeline);
            render_pass.set_bind_group(1, bind_group, &[]);
            for chunk in &*chunk_list.chunks {
                render_pass.set_immediates(0, Immediates {
                    position: chunk.position,
                    direction: enum_map::Enum::into_usize(chunk.direction).try_into().unwrap(),
                }.as_std140().as_bytes());
                render_pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
                render_pass.draw(0..4, 0..(chunk.vertex_buffer.size() / 4).try_into().unwrap());
            }
            render_pass.pop_debug_group();

            OutputValue(render_pass)
        };
    );

    render_graph.set_resource_input(texture_resource, cfg.texture.clone());
    render_graph.set_resource_input(transparency_resource, cfg.transparency);
    render_graph.set_resource_input(chunk_list_resource, ChunkList { chunks: Rc::default() });

    chunk_list_resource
}

pub struct ChunkMaterial {
    pub cfg: ChunkRenderConfig,
    pub chunk_list: Rc<[ChunkRenderData]>,
    chunk_list_resource: Option<ResourceHandle<ChunkList>>,
}

impl ChunkMaterial {
    pub fn new(cfg: ChunkRenderConfig) -> Self {
        Self {
            cfg,
            chunk_list: Rc::default(),
            chunk_list_resource: None,
        }
    }
}

impl Material for ChunkMaterial {
    fn register_global(render_graph: &mut engine::material::RenderGraphWrapper<'_>)
        where Self: Sized,
    {
        register_global(render_graph);
    }

    fn register(&mut self, render_graph: &mut engine::material::RenderGraphWrapper<'_>) {
        self.chunk_list_resource = Some(register(&self.cfg, render_graph));
    }

    fn update(&mut self, render_graph: &mut render_graph::RenderGraph) {
        render_graph.set_resource_input(self.chunk_list_resource.unwrap(), ChunkList { chunks: Rc::clone(&self.chunk_list) });
    }
}
