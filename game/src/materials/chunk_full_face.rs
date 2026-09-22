use crevice::std140::AsStd140;
use engine::{ wgpu, Material, render_graph_nodes as engine_graph, renderer::resources as render_res };
use glam::{ISizeVec3, Vec3};
use ordermap::{OrderMap, OrderSet};
use render_graph::ResourceHandle;
use resource::resource_str;

use super::{ EnableWireframes, CutoutRenderStep, OpaqueRenderStep, TranslucentRenderStep };
use crate::chunk_mesher::ChunkTransparencyMode;
use crate::utils::{ CardinalDirection, AABB3 };
use crate::chunk::CHUNK_SIZE;

render_graph::graph_resource!(struct ShaderSourceCode(wgpu::naga::Module); permanent);
render_graph::graph_resource!(struct ShaderModule(wgpu::ShaderModule); permanent);
render_graph::graph_resource!(struct BindGroupLayout(wgpu::BindGroupLayout); permanent);
render_graph::graph_resource!(struct RenderPipelineLayout(wgpu::PipelineLayout); permanent);

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
    chunk_pos: ISizeVec3,
    direction: CardinalDirection,
    position: Vec3,
    vertex_buffer: wgpu::Buffer,
}

impl ChunkRenderData {
    pub fn upload(
        device: &wgpu::Device,
        chunk_pos: ISizeVec3,
        submesh: &crate::chunk_mesher::QuadSubMesh,
    ) -> Self {
            let instances_bytes = bytemuck::cast_slice::<_, u8>(&submesh.instances);
            let buffer = device.create_buffer(&wgpu::wgt::BufferDescriptor {
                label: Some(&format!("chunk,{chunk_pos},{:?}", submesh.direction)),
                size: instances_bytes.len().try_into().unwrap(),
                usage: wgpu::BufferUsages::VERTEX,
                mapped_at_creation: true,
            });
            buffer.slice(..).get_mapped_range_mut().unwrap().copy_from_slice(instances_bytes);
            buffer.unmap();

            Self {
                direction: submesh.direction,
                chunk_pos,
                position: (chunk_pos * CHUNK_SIZE.as_isizevec3()).as_vec3(),
                vertex_buffer: buffer,
            }
    }
}

struct ChunkList {
    chunks: OrderMap<ISizeVec3, Vec<ChunkRenderData>>,
}

fn register_global(render_graph: &mut engine::RenderGraphWrapper<'_>) {
    render_graph::node_helper!(into render_graph;
        CreateShaderModule
        (device: ref render_res::Device, shader_code: ref ShaderSourceCode) -> (ShaderModule) {
            OutputValue(device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Chunk full face material"),
                source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(shader_code.clone())),
            }))
        };

        CreateBindGroupLayout
        (device: ref render_res::Device) -> (BindGroupLayout) {
            OutputValue(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Chunk full face material"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
    );
}

struct GlobalMaterial {
    shader_code: resource::Resource<str>,
}
impl engine::GlobalMaterial for GlobalMaterial {
    fn register(render_graph: &mut engine::RenderGraphWrapper<'_>) -> Self
        where Self: Sized,
    {
        let shader_code = resource_str!("src/materials/chunk_full_face_material.wgsl");
        match wgpu::naga::front::wgsl::parse_str(shader_code.as_ref()) {
            Ok(module) => render_graph.set_input::<ShaderSourceCode>(module),
            Err(error) => {
                eprintln!("{}", error.emit_to_string(shader_code.as_ref()));
                panic!();
            }
        }
        register_global(render_graph);
        Self {
            shader_code,
        }
    }

    fn update(&mut self, render_graph: &mut render_graph::RenderGraph) {
        if self.shader_code.reload_if_changed() {
            match wgpu::naga::front::wgsl::parse_str(self.shader_code.as_ref()) {
                Ok(module) => {
                    tracing::info!("Hot Reloaded chunk_material.wgsl");
                    render_graph.set_input::<ShaderSourceCode>(module);
                },
                Err(error) => {
                    tracing::error!(%error, "Error while compiling chunk_material.wgsl");
                    eprintln!("{}", error.emit_to_string(self.shader_code.as_ref()));
                },
            }
        }
    }
}

fn register(cfg: &ChunkRenderConfig, render_graph: &mut engine::RenderGraphWrapper<'_>) -> ResourceHandle<ChunkList> {
    use render_graph::ResourceConfig as Cfg;
    render_graph::node_helper!(into render_graph;
        using @chunk_list: ChunkList = render_graph.create_resource("chunk_material::chunk_list", Cfg::permanent());

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
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            });
            OutputValue(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Chunk full face material"),
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
                label: Some("Chunk full face material"),
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
                    cull_mode: (!enable_wireframes).then_some(wgpu::Face::Back),
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
            chunk_list: ref @chunk_list,

            &transparency: ref @transparency,
            _: ref @draw_step,
        ) -> (engine_graph::RenderPass) {
            let camera_position = world.camera_clip_transform.transform_point3(Vec3::ZERO);
            let view_projection = world.camera_projection * world.camera_clip_transform.inverse_or_zero();

            render_pass.push_debug_group(&format!("{transparency:?} Chunk full face renderer"));
            render_pass.set_pipeline(render_pipeline);
            render_pass.set_bind_group(1, bind_group, &[]);
            for chunk in chunk_list.chunks.values().flatten() {
                let max_pos = chunk.position + CHUNK_SIZE.as_vec3();
                let clip = if chunk.direction.sign.is_positive() {
                    camera_position[chunk.direction.axis] < chunk.position[chunk.direction.axis]
                } else {
                    camera_position[chunk.direction.axis] > max_pos[chunk.direction.axis]
                };
                if clip { continue }
                if !(AABB3 { min: chunk.position, max: max_pos }).frustrum_test(&view_projection) { continue }
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

#[expect(clippy::module_name_repetitions, reason = "Needed here to not clash with Material trait")]
pub struct ChunkFullFaceMaterial {
    cfg: ChunkRenderConfig,
    remove_chunks: OrderSet<ISizeVec3>,
    new_chunks: OrderMap<ISizeVec3, Vec<ChunkRenderData>>,
    chunk_list_resource: Option<ResourceHandle<ChunkList>>,
}

impl ChunkFullFaceMaterial {
    pub fn new(cfg: ChunkRenderConfig) -> Self {
        Self {
            cfg,
            remove_chunks: OrderSet::new(),
            new_chunks: OrderMap::new(),
            chunk_list_resource: None,
        }
    }

    pub fn remove_chunk(&mut self, pos: ISizeVec3) {
        self.remove_chunks.insert(pos);
        self.new_chunks.remove(&pos);
    }

    pub fn insert_chunk(&mut self, data: ChunkRenderData) {
        self.new_chunks.entry(data.chunk_pos)
            .or_default().push(data);
    }
}

impl Material for ChunkFullFaceMaterial {
    fn global_materials() -> impl engine::GlobalMaterialList
        where Self: Sized
    {
        engine::GlobalMaterialTuple::<(GlobalMaterial, super::RenderGlobalMaterial)>::new()
    }

    fn register(&mut self, render_graph: &mut engine::RenderGraphWrapper<'_>) {
        self.chunk_list_resource = Some(register(&self.cfg, render_graph));
    }

    fn update(&mut self, render_graph: &mut render_graph::RenderGraph) {
        let chunk_list_resource = self.chunk_list_resource.unwrap();
        let chunk_list = render_graph.compute_resource(chunk_list_resource).unwrap().into_mut();
        let changed = !self.remove_chunks.is_empty() || !self.new_chunks.is_empty();
        if !changed { return; }
        for pos in self.remove_chunks.drain(..) {
            chunk_list.chunks.remove(&pos);
        }
        for new in self.new_chunks.drain(..).flat_map(|(_,c)| c) {
            chunk_list.chunks.entry(new.chunk_pos)
                .or_default().push(new);
        }
        render_graph.mark_resource_input_dirty(chunk_list_resource);
    }
}
