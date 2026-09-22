pub mod chunk_full_face;
pub mod chunk;
pub mod block_selection;

render_graph::graph_resource!(pub struct EnableWireframes(pub bool); permanent);

render_graph::graph_resource!(struct OpaqueRenderStep(()));
render_graph::graph_resource!(struct CutoutRenderStep(()));
render_graph::graph_resource!(struct TranslucentRenderStep(()));

fn register_global(render_graph: &mut engine::RenderGraphWrapper<'_>) {
    render_graph::node_helper!(into render_graph;
        BeginOpaqueStep() -> (default OpaqueRenderStep);
        BeginCutoutStep(_: OpaqueRenderStep) -> (default CutoutRenderStep);
        BeginTranslucentStep(_: CutoutRenderStep) -> (default TranslucentRenderStep);
    );
}

struct RenderGlobalMaterial;
impl engine::GlobalMaterial for RenderGlobalMaterial {
    fn register(render_graph: &mut engine::RenderGraphWrapper<'_>) -> Self
        where Self: Sized,
    {
        register_global(render_graph);
        Self
    }
}
