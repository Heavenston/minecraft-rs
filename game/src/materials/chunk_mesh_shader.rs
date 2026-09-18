#![expect(dead_code, reason = "wip")]

use crevice::std140::{AsStd140, Std140 as _};
use engine::{ wgpu, Material, render_graph_nodes as engine_graph, renderer::resources as render_res };
use glam::ISizeVec3;
use itertools::{ Itertools as _ };
use render_graph::ResourceHandle;
use resource::resource_str;

use super::{ EnableWireframes, CutoutRenderStep, OpaqueRenderStep, TranslucentRenderStep };
use crate::chunk_mesher::{self, ChunkTransparencyMode};
use crate::chunk::CHUNK_SIZE;

const MAX_CHUNK_COUNT: usize = 0x0010_0000;
pub const MAX_MODEL_COUNT: usize = 0x0001_0000;
pub const MODEL_BUFFER_SIZE: usize = MAX_MODEL_COUNT * size_of::<chunk_mesher::mesh_shader::BlockModel>();

render_graph::graph_resource!(struct ShaderSourceCode(wgpu::naga::Module); permanent);
render_graph::graph_resource!(struct ShaderModule(wgpu::ShaderModule); permanent);
render_graph::graph_resource!(struct BindGroupLayout(wgpu::BindGroupLayout); permanent);
render_graph::graph_resource!(struct ChunkListBindGroupLayout(wgpu::BindGroupLayout); permanent);
render_graph::graph_resource!(struct RenderPipelineLayout(wgpu::PipelineLayout); permanent);

pub struct RenderConfig {
    pub texture: wgpu::Texture,
    pub models: wgpu::Buffer,
    pub transparency: ChunkTransparencyMode,
}

#[derive(AsStd140)]
struct Immediates {
    chunk_idx: u32,
}

#[derive(Debug, Clone)]
struct PerChunkRenderData {
    buffer: wgpu::Buffer,
}

struct ChunkList {
    chunks: Vec<PerChunkRenderData>,
}

fn register_global(render_graph: &mut engine::RenderGraphWrapper<'_>) {
    render_graph::node_helper!(into render_graph;
        CreateShaderModule
        (device: ref render_res::Device, shader_code: ref ShaderSourceCode) -> (ShaderModule) {
            let descriptor = wgpu::ShaderModuleDescriptor {
                label: Some("Chunk mesh shader"),
                source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(shader_code.clone())),
            };
            if cfg!(debug_assertions) {
                OutputValue(device.create_shader_module(descriptor))
            }
            else {
                OutputValue(unsafe { device.create_shader_module_trusted(descriptor, wgpu::ShaderRuntimeChecks::unchecked()) })
            }
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
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

        CreateChunkListBindGroupLayout
        (device: ref render_res::Device) -> (ChunkListBindGroupLayout) {
            OutputValue(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Chunk mesh shader, chunk data list"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::MESH,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: Some(std::num::NonZero::new(MAX_CHUNK_COUNT.try_into().unwrap()).unwrap()),
                    },
                ],
            }))
        };

        CreateRenderPipelineLayout (
            device: ref render_res::Device,
            world_bind_group_layout: ref engine_graph::WorldBindGroupLayout,
            bind_group_layout: ref BindGroupLayout,
            chunk_bind_group_layout: ref ChunkListBindGroupLayout,
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

        use wgpu::naga::valid::{ Validator, ValidationFlags };

        let capabilities = wgpu::wgc::device::features_to_naga_capabilities(ChunkMeshShaderMaterial::required_features(), wgpu::DownlevelFlags::compliant());
        let mut validator = Validator::new(ValidationFlags::all(), capabilities);
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
    buffer: wgpu::Buffer,
}

impl ChunkBuffers {
    pub fn upload_mesh(device: &wgpu::Device, pos: ISizeVec3, mesh: &chunk_mesher::mesh_shader::Mesh) -> Self {
        assert_eq!(mesh.data.len(), CHUNK_SIZE.element_product());
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("Chunk {pos} data buffer")),
            size: u64::try_from(mesh.data.len() * 4 + 16).unwrap(),
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: true,
        });

        let mut view = buffer.get_mapped_range_mut(..).unwrap();
        view.slice(..12).copy_from_slice((pos * CHUNK_SIZE.as_isizevec3()).as_vec3().as_std140().as_bytes());
        view.slice(16..).copy_from_slice(bytemuck::cast_slice::<_, u8>(&mesh.data[..]));
        drop(view);
        
        buffer.unmap();
        Self { buffer }
    }
}

fn register(cfg: &RenderConfig, render_graph: &mut engine::RenderGraphWrapper<'_>) -> ResourceHandle<ChunkList> {
    use render_graph::ResourceConfig as Cfg;
    render_graph::node_helper!(into render_graph;
        using @chunk_list: ChunkList = render_graph.create_resource("chunk_mesh_shader_material::chunk_list", Cfg::permanent());

        using @transparency: ChunkTransparencyMode = render_graph.create_resource("chunk_mesh_shader_material::transparency", Cfg::permanent());
        using @texture: wgpu::Texture = render_graph.create_resource("chunk_mesh_shader_material::texture", Cfg::permanent());
        using @models: wgpu::Buffer = render_graph.create_resource("chunk_mesh_shader_material::models", Cfg::permanent());

        using @bind_group: wgpu::BindGroup = render_graph.create_resource("chunk_mesh_shader_material::bind_group", Cfg::permanent());
        using @chunk_list_bind_group: wgpu::BindGroup = render_graph.create_resource("chunk_mesh_shader_material::chunk_list_bind_group", Cfg::permanent());
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
            models: ref @models,
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
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: models.as_entire_binding(),
                    },
                ],
            }))
        };

        CreateChunkListBindGroup (
            device: ref render_res::Device,
            chunk_list_bind_group_layout: ref ChunkListBindGroupLayout,
            chunk_list: ref @chunk_list,
        ) -> (@chunk_list_bind_group) {
            assert!(!chunk_list.chunks.is_empty(), "no chunk to render");
            let bindings = chunk_list.chunks.iter().map(|chunk| chunk.buffer.as_entire_buffer_binding()).collect_vec();
            OutputValue(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Chunk mesh shader global"),
                layout: chunk_list_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::BufferArray(&bindings),
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
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[],
                        zero_initialize_workgroup_memory: false,
                    },
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    // Culling is done by the mesh shader
                    cull_mode: None,
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
            bind_group: ref @bind_group,
            chunk_list_bind_group: ref @chunk_list_bind_group,
            render_pipeline: ref @render_pipeline,
            chunk_list: ref @chunk_list,

            &transparency: ref @transparency,
            _: ref @draw_step,
        ) -> (engine_graph::RenderPass) {
            render_pass.push_debug_group(&format!("{transparency:?} Chunk mesh shader renderer"));
            render_pass.set_pipeline(render_pipeline);
            render_pass.set_bind_group(1, bind_group, &[]);
            render_pass.set_bind_group(2, chunk_list_bind_group, &[]);
            for chunk_idx in 0..u32::try_from(chunk_list.chunks.len()).unwrap() {
                render_pass.set_immediates(0, Immediates {
                    chunk_idx,
                }.as_std140().as_bytes());
                render_pass.draw_mesh_tasks(8,8,16);
            }
            render_pass.pop_debug_group();

            OutputValue(render_pass)
        };
    );

    render_graph.set_resource_input(texture_resource, cfg.texture.clone());
    render_graph.set_resource_input(models_resource, cfg.models.clone());
    render_graph.set_resource_input(transparency_resource, cfg.transparency);
    render_graph.set_resource_input(chunk_list_resource, ChunkList { chunks: Default::default() });

    chunk_list_resource
}

#[expect(clippy::module_name_repetitions, reason = "Needed here to not clash with Material trait")]
pub struct ChunkMeshShaderMaterial {
    pub cfg: RenderConfig,
    new_chunk_list: Vec<ChunkBuffers>,
    chunk_list_resource: Option<ResourceHandle<ChunkList>>,

    chunk_count: usize,
}

impl ChunkMeshShaderMaterial {
    fn required_features() -> wgpu::Features {
        wgpu::Features::IMMEDIATES                     |
        wgpu::Features::EXPERIMENTAL_MESH_SHADER       |
        wgpu::Features::BUFFER_BINDING_ARRAY           |
        wgpu::Features::STORAGE_RESOURCE_BINDING_ARRAY |
        wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY
    }

    pub fn is_suported(device: &wgpu::Device) -> bool {
        device.features().contains(Self::required_features())
    }

    pub fn new(cfg: RenderConfig) -> Self {
        assert_eq!(cfg.models.size(), u64::try_from(MODEL_BUFFER_SIZE).unwrap());
        Self {
            cfg,
            new_chunk_list: vec![],
            chunk_list_resource: None,

            chunk_count: 0,
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
        let chunk_list_resource = self.chunk_list_resource.expect("should have been registered");

        if !self.new_chunk_list.is_empty() {
            let chunk_list = render_graph.compute_resource(chunk_list_resource).into_mut();
            for ChunkBuffers { buffer } in self.new_chunk_list.drain(..) {
                self.chunk_count += 1;
                chunk_list.chunks.push(PerChunkRenderData { buffer });
            }
            render_graph.mark_resource_input_dirty(chunk_list_resource);
        }

        assert_ne!(self.chunk_count, 0);
    }
}
