use crevice::std140::AsStd140;
use engine::{ wgpu, Material, render_graph_nodes as engine_graph, renderer::resources as render_res };
use glam::Vec3;
use render_graph::ResourceHandle;
use resource::resource_str;

use crate::materials::TranslucentRenderStep;

render_graph::graph_resource!(struct ShaderSourceCode(wgpu::naga::Module); permanent);
render_graph::graph_resource!(struct ShaderModule(wgpu::ShaderModule); permanent);
render_graph::graph_resource!(struct RenderPipelineLayout(wgpu::PipelineLayout); permanent);

#[derive(AsStd140)]
struct Immediates {
    position: Vec3,
}

struct Position {
    pos: Option<Vec3>,
}

fn register_global(render_graph: &mut engine::RenderGraphWrapper<'_>) {
    render_graph::node_helper!(into render_graph;
        CreateShaderModule
        (device: ref render_res::Device, shader_code: ref ShaderSourceCode) -> (ShaderModule) {
            OutputValue(device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Block Selection Material"),
                source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(shader_code.clone())),
            }))
        };

        CreateRenderPipelineLayout (
            device: ref render_res::Device,
            world_bind_group_layout: ref engine_graph::WorldBindGroupLayout,
        ) -> (RenderPipelineLayout) {
            OutputValue(device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Block Selection Material"),
                bind_group_layouts: &[Some(world_bind_group_layout)],
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
        let shader_code = resource_str!("src/materials/block_selection.wgsl");
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
                    tracing::info!("Hot Reloaded block_selection.wgsl");
                    render_graph.set_input::<ShaderSourceCode>(module);
                },
                Err(error) => {
                    tracing::error!(%error, "Error while compiling block_selection.wgsl");
                    eprintln!("{}", error.emit_to_string(self.shader_code.as_ref()));
                },
            }
        }
    }
}

fn register(render_graph: &mut engine::RenderGraphWrapper<'_>) -> ResourceHandle<Position> {
    use render_graph::ResourceConfig as Cfg;
    render_graph::node_helper!(into render_graph;
        using @position: Position = render_graph.create_resource("block_selection::position", Cfg::new());
        using @render_pipeline: wgpu::RenderPipeline = render_graph.create_resource("block_selection::render_pipeline", Cfg::permanent());
        using @draw_step: () = render_graph.resource_from_type::<TranslucentRenderStep>();

        CreateRenderPipeline(
            device: ref render_res::Device,
            shader_module: ref ShaderModule, render_pipeline_layout: ref RenderPipelineLayout,
        ) -> (@render_pipeline) {
            OutputValue(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Block Selection Material"),
                layout: Some(render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader_module,
                    entry_point: None,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: shader_module,
                    entry_point: None,
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[],
                        ..wgpu::PipelineCompilationOptions::default()
                    },
                    targets: &[
                        Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Bgra8UnormSrgb,
                            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            render_pipeline: ref @render_pipeline,
            position: ref @position,

            _: ref @draw_step,
        ) -> (engine_graph::RenderPass) {
            if let Some(position) = position.pos {
                render_pass.push_debug_group("Block Selection Renderer");
                render_pass.set_pipeline(render_pipeline);
                render_pass.set_immediates(0, Immediates { position }.as_std140().as_bytes());
                render_pass.draw(0..36, 0..1);
                render_pass.pop_debug_group();
            }

            OutputValue(render_pass)
        };
    );

    position_resource
}

pub struct BlockSelection {
    position: Option<Vec3>,
    position_resource: Option<ResourceHandle<Position>>,
}

impl BlockSelection {
    pub fn new() -> Self {
        Self {
            position: None,
            position_resource: None,
        }
    }

    pub fn set_position(&mut self, pos: Option<Vec3>) {
        self.position = pos;
    }
}

impl Material for BlockSelection {
    fn global_materials() -> impl engine::GlobalMaterialList
        where Self: Sized
    {
        engine::GlobalMaterialTuple::<(GlobalMaterial, super::RenderGlobalMaterial)>::new()
    }

    fn register(&mut self, render_graph: &mut engine::RenderGraphWrapper<'_>) {
        self.position_resource = Some(register(render_graph));
    }

    fn update(&mut self, render_graph: &mut render_graph::RenderGraph) {
        render_graph.set_resource_input(self.position_resource.unwrap(), Position { pos: self.position });
    }
}
