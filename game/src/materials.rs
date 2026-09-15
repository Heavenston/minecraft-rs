use glam::{Mat4, Vec3, Vec4};

pub mod chunk_full_face;
pub mod chunk;

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
