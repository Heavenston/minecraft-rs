use std::borrow::Cow;
use render_graph::RenderGraph;

use crate::material_store::{GlobalMaterialList, GlobalMaterialTuple};

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

    pub fn resource_from_type<R: render_graph::GraphResourceId>(&mut self) -> render_graph::ResourceHandle<R::Resource> {
        self.render_graph.resource_from_type::<R>()
    }

    pub fn create_resource<S: std::any::Any>(&mut self, label: impl Into<Cow<'static, str>>, config: render_graph::ResourceConfig) -> render_graph::ResourceHandle<S> {
        self.render_graph.create_resource::<S>(label.into(), config)
    }

    pub fn push_node<N: render_graph::GraphNode>(&mut self, node: N) -> render_graph::NodeHandle<N>
        where N::InputBundle: Default,
              N::OutputBundle: Default,
    {
        let handle = self.render_graph.push_node(node);
        self.added_nodes.push(handle.to_untyped());
        handle
    }

    pub fn push_node_complete<N: render_graph::GraphNode>(&mut self, node: N, input_bundle: N::InputBundle, output_bundle: N::OutputBundle) -> render_graph::NodeHandle<N> {
        let handle = self.render_graph.push_node_complete(node, input_bundle, output_bundle);
        self.added_nodes.push(handle.to_untyped());
        handle
    }

    pub fn set_input<R: render_graph::GraphResourceId>(&mut self, val: R::Resource) {
        self.render_graph.set_input::<R>(val);
    }

    pub fn set_resource_input<R: std::any::Any>(&mut self, handle: render_graph::ResourceHandle<R>, value: R) {
        self.render_graph.set_resource_input(handle, value);
    }

    pub fn set_resource_input_untyped(&mut self, handle: render_graph::UntypedResourceHandle, value: Box<dyn std::any::Any>) {
        self.render_graph.set_resource_input_untyped(handle, value);
    }
}

pub trait GlobalMaterial: std::any::Any + Send + Sync {
    fn register(render_graph: &mut RenderGraphWrapper<'_>) -> Self
        where Self: Sized;
    fn update(&mut self, render_graph: &mut RenderGraph) {
        let _ = render_graph;
    }
}

pub trait Material: std::any::Any + Send + Sync {
    fn global_materials() -> impl GlobalMaterialList
        where Self: Sized {
        GlobalMaterialTuple::<()>::new()
    }

    fn register(&mut self, render_graph: &mut RenderGraphWrapper<'_>);
    fn update(&mut self, render_graph: &mut RenderGraph) {
        let _ = render_graph;
    }
}
