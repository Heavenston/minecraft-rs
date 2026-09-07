mod rotating;
pub use rotating::*;

use render_graph::RenderGraph;

pub struct RenderGraphWrapper<'a> {
    render_graph: &'a mut RenderGraph,
    added_nodes: Vec<render_graph::UntypedNodeHandle>,
}

impl<'a> RenderGraphWrapper<'a> {
    pub fn new(render_graph: &'a mut RenderGraph) -> Self {
        Self {
            render_graph,
            added_nodes: vec![],
        }
    }

    pub fn finish(self) -> Vec<render_graph::UntypedNodeHandle> {
        self.added_nodes
    }

    pub fn create_resource<S: std::any::Any>(&mut self, is_permanent: bool) -> render_graph::ResourceHandle<S> {
        self.render_graph.create_resource::<S>(is_permanent)
    }

    pub fn create_resource_untyped(&mut self, label: &'static str, storage: std::any::TypeId, is_permanent: bool) -> render_graph::UntypedResourceHandle {
        self.render_graph.create_resource_untyped(label, storage, is_permanent)
    }

    pub fn push_node<N: render_graph::GraphNode>(&mut self, node: N) -> render_graph::NodeHandle<N>
        where N::InputBundle: Default,
              N::OutputBundle: Default,
    {
        let handle = self.render_graph.push_node(node);
        self.added_nodes.push(handle.to_untyped());
        handle
    }
}

pub trait Material: std::any::Any {
    fn register(&mut self, render_graph: &mut RenderGraphWrapper<'_>);
}
