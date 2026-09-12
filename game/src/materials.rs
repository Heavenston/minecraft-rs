use std::sync::Arc;
use crevice::std140::AsStd140;
use engine::{ wgpu, Material, render_graph_nodes as engine_graph, renderer::resources as render_res };
use glam::{Mat4, Vec3, Vec4};
use render_graph::ResourceHandle;

use crate::chunk_mesher::ChunkTransparencyMode;
use crate::utils::CardinalDirection;
use crate::chunk::CHUNK_SIZE;

static SHADER_CODE: &str = include_str!("chunk_material.wgsl");

render_graph::graph_resource!(pub struct EnableWireframes(pub bool); permanent);

render_graph::graph_resource!(pub struct ShaderModule(pub wgpu::ShaderModule); permanent);
render_graph::graph_resource!(pub struct BindGroupLayout(pub wgpu::BindGroupLayout); permanent);
render_graph::graph_resource!(pub struct RenderPipelineLayout(pub wgpu::PipelineLayout); permanent);

render_graph::graph_resource!(pub struct OpaqueRenderStep(pub ()));
render_graph::graph_resource!(pub struct CutoutRenderStep(pub ()));
render_graph::graph_resource!(pub struct TranslucentRenderStep(pub ()));

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
    chunks: Arc<[ChunkRenderData]>,
}

fn test_aabb_against_frustum(mvp: &Mat4, min: Vec3, max: Vec3) -> bool {
    // Use our min max to define eight corners
    let corners: [Vec4; 8] = [
        Vec4::new(min.x, min.y, min.z, 1.0), // x y z
        Vec4::new(max.x, min.y, min.z, 1.0), // X y z
        Vec4::new(min.x, max.y, min.z, 1.0), // x Y z
        Vec4::new(max.x, max.y, min.z, 1.0), // X Y z

        Vec4::new(min.x, min.y, max.z, 1.0), // x y Z
        Vec4::new(max.x, min.y, max.z, 1.0), // X y Z
        Vec4::new(min.x, max.y, max.z, 1.0), // x Y Z
        Vec4::new(max.x, max.y, max.z, 1.0), // X Y Z
    ];
    let corners = corners.map(|corner| mvp * corner);

    !(
        // left and right
        (corners.iter().all(|corner| corner.x < -corner.w) || corners.iter().all(|corner| corner.x > corner.w)) &&
        // bottom and top
        (corners.iter().all(|corner| corner.y < -corner.w) || corners.iter().all(|corner| corner.y > corner.w)) &&
        // near and far
        (corners.iter().all(|corner| corner.z < 0.) || corners.iter().all(|corner| corner.z > corner.w))
    )
}

fn register_global(render_graph: &mut engine::RenderGraphWrapper<'_>) {
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
                            view_dimension: wgpu::TextureViewDimension::D2Array,
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

fn register(cfg: &ChunkRenderConfig, render_graph: &mut engine::RenderGraphWrapper<'_>) -> ResourceHandle<ChunkList> {
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
            world: ref engine_graph::WorldResource,
            bind_group: ref @bind_group,
            render_pipeline: ref @render_pipeline,
            chunk_list: @chunk_list,

            &transparency: ref @transparency,
            _: ref @draw_step,
        ) -> (engine_graph::RenderPass) {
            let camera_position = world.camera_clip_transform.transform_point3(Vec3::ZERO);
            let view_projection = world.camera_projection * world.camera_clip_transform.inverse_or_zero();

            render_pass.push_debug_group(&format!("{transparency:?} Chunk renderer"));
            render_pass.set_pipeline(render_pipeline);
            render_pass.set_bind_group(1, bind_group, &[]);
            for chunk in &*chunk_list.chunks {
                let max_pos = chunk.position + CHUNK_SIZE.as_vec3();
                let clip = if chunk.direction.is_positive() {
                    camera_position[chunk.direction.axis()] < chunk.position[chunk.direction.axis()]
                } else {
                    camera_position[chunk.direction.axis()] > max_pos[chunk.direction.axis()]
                };
                if clip { continue }
                if !test_aabb_against_frustum(&view_projection, chunk.position, max_pos) { continue }
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
    render_graph.set_resource_input(chunk_list_resource, ChunkList { chunks: Default::default() });

    chunk_list_resource
}

pub struct ChunkMaterial {
    pub cfg: ChunkRenderConfig,
    pub chunk_list: Arc<[ChunkRenderData]>,
    chunk_list_resource: Option<ResourceHandle<ChunkList>>,
}

impl ChunkMaterial {
    pub fn new(cfg: ChunkRenderConfig) -> Self {
        Self {
            cfg,
            chunk_list: Default::default(),
            chunk_list_resource: None,
        }
    }
}

impl Material for ChunkMaterial {
    fn register_global(render_graph: &mut engine::RenderGraphWrapper<'_>)
        where Self: Sized,
    {
        register_global(render_graph);
    }

    fn register(&mut self, render_graph: &mut engine::RenderGraphWrapper<'_>) {
        self.chunk_list_resource = Some(register(&self.cfg, render_graph));
    }

    fn update(&mut self, render_graph: &mut render_graph::RenderGraph) {
        render_graph.set_resource_input(self.chunk_list_resource.unwrap(), ChunkList { chunks: Arc::clone(&self.chunk_list) });
    }
}
