#![expect(dead_code, reason = "wip")]

use crevice::std140::AsStd140;
use engine::{ wgpu, Material, render_graph_nodes as engine_graph, renderer::resources as render_res };
use glam::{ISizeVec3, Vec3};
use render_graph::ResourceHandle;
use resource::resource_str;
use wgpu::util::DeviceExt as _;

use super::{ test_aabb_against_frustum, EnableWireframes, CutoutRenderStep, OpaqueRenderStep, TranslucentRenderStep };
use crate::chunk_mesher::{self, ChunkTransparencyMode};
use crate::chunk::CHUNK_SIZE;

render_graph::graph_resource!(struct ShaderSourceCode(wgpu::naga::Module); permanent);
render_graph::graph_resource!(struct ShaderModule(wgpu::ShaderModule); permanent);
render_graph::graph_resource!(struct BindGroupLayout(wgpu::BindGroupLayout); permanent);
render_graph::graph_resource!(struct ChunkBindGroupLayout(wgpu::BindGroupLayout); permanent);
render_graph::graph_resource!(struct RenderPipelineLayout(wgpu::PipelineLayout); permanent);

pub struct RenderConfig {
    pub texture: wgpu::Texture,
    pub transparency: ChunkTransparencyMode,
}

#[derive(AsStd140)]
struct Immediates {
    position: Vec3,
}

#[derive(Debug, Clone)]
pub struct PerChunkRenderData {
    pub position: Vec3,
    pub bind_group: wgpu::BindGroup,
}

struct ChunkList {
    chunks: Vec<PerChunkRenderData>,
}

fn register_global(render_graph: &mut engine::RenderGraphWrapper<'_>) {
    render_graph::node_helper!(into render_graph;
        CreateShaderModule
        (device: ref render_res::Device, shader_code: ref ShaderSourceCode) -> (ShaderModule) {
            OutputValue(unsafe { device.create_shader_module_trusted(wgpu::ShaderModuleDescriptor {
                label: Some("Chunk mesh shader"),
                source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(shader_code.clone())),
            }, wgpu::ShaderRuntimeChecks::unchecked()) })
        };

        CreateBindGroupLayout
        (device: ref render_res::Device) -> (BindGroupLayout) {
            OutputValue(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Chunk mesh shader, global"),
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

        CreateChunkBindGroupLayout
        (device: ref render_res::Device) -> (ChunkBindGroupLayout) {
            OutputValue(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Chunk mesh shader, per chunk"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::MESH,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::MESH,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
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
            chunk_bind_group_layout: ref ChunkBindGroupLayout,
        ) -> (RenderPipelineLayout) {
            OutputValue(device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Chunk mesh shader"),
                bind_group_layouts: &[Some(world_bind_group_layout), Some(bind_group_layout), Some(chunk_bind_group_layout)],
                immediate_size: Immediates::std140_size_static().try_into().unwrap(),
            }))
        };
    );
}

struct GlobalMaterial {
    shader_code: resource::Resource<str>,
}
impl GlobalMaterial {
    fn get_module(&self) -> Option<wgpu::naga::Module> {
        let module = match wgpu::naga::front::wgsl::parse_str(self.shader_code.as_ref()) {
            Ok(module) => module,
            Err(error) => {
                tracing::error!(%error, "Error while compiling chunk_mesh_shader.wgsl");
                eprintln!("{}", error.emit_to_string_with_path(self.shader_code.as_ref(), "src/materials/chunk_mesh_shader.wgsl"));
                return None;
            },
        };
        if !cfg!(debug_assertions) { return Some(module); }

        use wgpu::naga::valid::{ Validator, ValidationFlags, Capabilities };

        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::IMMEDIATES | Capabilities::MESH_SHADER);
        match validator.validate(&module) {
            Ok(_) => (),
            Err(error) => {
                eprintln!("{}", error.emit_to_string_with_path(self.shader_code.as_ref(), "src/materials/chunk_mesh_shader.wgsl"));
                return None;
            },
        }

        Some(module)
    }
}
impl engine::GlobalMaterial for GlobalMaterial {
    fn register(render_graph: &mut engine::RenderGraphWrapper<'_>) -> Self
        where Self: Sized,
    {
        let this = Self { shader_code: resource_str!("src/materials/chunk_mesh_shader.wgsl") };
        render_graph.set_input::<ShaderSourceCode>(this.get_module().expect("Error making initial compilation for shader"));
        register_global(render_graph);
        this
    }

    fn update(&mut self, render_graph: &mut render_graph::RenderGraph) {
        if self.shader_code.reload_if_changed() && let Some(module) = self.get_module() {
            tracing::info!("Hot Reloaded chunk_mesh_shader.wgsl");
            render_graph.set_input::<ShaderSourceCode>(module);
        }
    }
}

pub struct ChunkBuffers {
    pub pos: ISizeVec3,
    pub data: wgpu::Buffer,
    pub models: wgpu::Buffer,
}

impl ChunkBuffers {
    pub fn upload_mesh(device: &wgpu::Device, pos: ISizeVec3, mesh: &chunk_mesher::mesh_shader::Mesh) -> Self {
        let data = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Chunk {pos} data buffer")),
            contents: bytemuck::cast_slice::<_, u8>(&mesh.data),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let models = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Chunk {pos} models buffer")),
            contents: bytemuck::cast_slice::<_, u8>(&mesh.models),
            usage: wgpu::BufferUsages::STORAGE,
        });
        Self { pos, data, models }
    }
}

fn register(cfg: &RenderConfig, render_graph: &mut engine::RenderGraphWrapper<'_>) -> ResourceHandle<ChunkList> {
    use render_graph::ResourceConfig as Cfg;
    render_graph::node_helper!(into render_graph;
        using @chunk_list: ChunkList = render_graph.create_resource("chunk_mesh_shader_material::chunk_list", Cfg::permanent());

        using @transparency: ChunkTransparencyMode = render_graph.create_resource("chunk_mesh_shader_material::transparency", Cfg::permanent());
        using @texture: wgpu::Texture = render_graph.create_resource("chunk_mesh_shader_material::texture", Cfg::permanent());

        using @bind_group: wgpu::BindGroup = render_graph.create_resource("chunk_mesh_shader_material::bind_group", Cfg::permanent());
        using @render_pipeline: wgpu::RenderPipeline = render_graph.create_resource("chunk_mesh_shader_material::render_pipeline", Cfg::permanent());

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
                label: Some("Chunk mesh shader global"),
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
            OutputValue(device.create_mesh_pipeline(&wgpu::MeshPipelineDescriptor {
                label:Some("Chunk material"),
                layout:Some(render_pipeline_layout),
                task: None,
                mesh: wgpu::MeshState {
                    module: shader_module,
                    entry_point: None,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
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
                multiview: None,
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
            let view_projection = world.camera_projection * world.camera_clip_transform.inverse_or_zero();

            render_pass.push_debug_group(&format!("{transparency:?} Chunk mesh shader renderer"));
            render_pass.set_pipeline(render_pipeline);
            render_pass.set_bind_group(1, bind_group, &[]);
            for chunk in &*chunk_list.chunks {
                let max_pos = chunk.position + CHUNK_SIZE.as_vec3();
                if !test_aabb_against_frustum(&view_projection, chunk.position, max_pos) { continue }
                render_pass.set_immediates(0, Immediates {
                    position: chunk.position,
                }.as_std140().as_bytes());
                render_pass.set_bind_group(2, &chunk.bind_group, &[]);
                render_pass.draw_mesh_tasks(16,16,16);
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
pub struct ChunkMeshShaderMaterial {
    pub cfg: RenderConfig,
    new_chunk_list: Vec<ChunkBuffers>,
    chunk_list_resource: Option<ResourceHandle<ChunkList>>,
}

impl ChunkMeshShaderMaterial {
    pub fn new(cfg: RenderConfig) -> Self {
        Self {
            cfg,
            new_chunk_list: vec![],
            chunk_list_resource: None,
        }
    }

    pub fn add_chunk(&mut self, chunk: ChunkBuffers) {
        self.new_chunk_list.push(chunk);
    }
}

impl Material for ChunkMeshShaderMaterial {
    fn global_materials() -> impl engine::GlobalMaterialList
        where Self: Sized
    {
        engine::GlobalMaterialTuple::<(GlobalMaterial, super::RenderGlobalMaterial)>::new()
    }

    fn register(&mut self, render_graph: &mut engine::RenderGraphWrapper<'_>) {
        self.chunk_list_resource = Some(register(&self.cfg, render_graph));
    }

    fn update(&mut self, render_graph: &mut render_graph::RenderGraph) {
        let Some(chunk_list_resource) = self.chunk_list_resource
        else { return };

        let device = render_graph.compute::<render_res::Device>().as_ref().clone();
        let chunk_bind_group_layout = render_graph.compute::<ChunkBindGroupLayout>().into_ref().clone();
        let chunk_list = render_graph.compute_resource(chunk_list_resource).into_mut();

        for ChunkBuffers { pos, data, models } in self.new_chunk_list.drain(..) {
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("Chunk {pos} for mesh shader")),
                layout: &chunk_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(data.as_entire_buffer_binding()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(models.as_entire_buffer_binding()),
                    },
                ],
            });
            chunk_list.chunks.push(PerChunkRenderData {
                position: (pos * CHUNK_SIZE.as_isizevec3()).as_vec3(),
                bind_group,
            });
        }
    }
}
