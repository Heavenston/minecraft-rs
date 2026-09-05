mod bundles;
pub use bundles::*;
mod node;
pub use node::*;
// mod compiler;
#[cfg(test)]
mod tests;

use std::{any::{ Any, TypeId }, collections::{HashMap, HashSet}, marker::PhantomData};
use static_assertions as sa;
use genmap::{GenMap, Handle};
use itertools::Itertools as _;

// use crate::compiler::CompiledGraph;
struct CompiledGraph { }
impl CompiledGraph {
    fn new(_graph: &RenderGraph) -> Self { Self { } }
}

#[derive(Debug, Clone, Default)]
pub struct TypeNameRegistry {
    content: HashMap<TypeId, &'static str>
}

impl TypeNameRegistry {
    pub fn register<T: Any>(&mut self) {
        self.content.insert(TypeId::of::<T>(), std::any::type_name::<T>());
    }

    pub fn get_name(&self, id: TypeId) -> &'static str {
        self.content.get(&id).copied().unwrap_or("<unknown>")
    }

    pub fn try_get_name(&self, id: TypeId) -> Option<&'static str> {
        self.content.get(&id).copied()
    }
}

pub trait GraphResourceId: Any {
    type Resource
        where Self: Sized;
    fn is_permanent() -> bool
        where Self: Sized;
    fn new_resource(val: Self::Resource) -> Self
        where Self: Sized;
    fn get_resource(self) -> Self::Resource
        where Self: Sized;
    fn get_resource_ref(&self) -> &Self::Resource
        where Self: Sized;
}
typemap::impl_dyn_trait!(GraphResourceId);

#[macro_export]
macro_rules! graph_resource {
    (@permanence) => { false };
    (@permanence; permanent) => { true };
    ($(#[$attrs:meta])* $vis:vis struct $name:ident($val_vis:vis $content:ty)$($parameters:tt)*) => {
        $(#[$attrs])* $vis struct $name($val_vis $content);
        impl $crate::GraphResourceId for $name {
            type Resource = $content
                where Self: Sized;
            fn is_permanent() -> bool
                where Self: Sized,
            {
                $crate::graph_resource!(@permanence $($parameters)*)
            }
            fn new_resource(val: Self::Resource) -> Self
                where Self: Sized
            {
                Self(val)
            }
            fn get_resource(self) -> Self::Resource
                where Self: Sized
            {
                self.0
            }
            fn get_resource_ref(&self) -> &Self::Resource
                where Self: Sized
            {
                &self.0
            }
        }
    };
}

pub trait ResourceInfoProvider {
    fn resource_from_type<R: GraphResourceId>(&mut self) -> ResourceHandle<R::Resource>;
}

pub trait ResourceGatherer: ResourceInfoProvider {
    fn consume<R: Any>(&mut self, handle: ResourceHandle<R>) -> R;
    fn consume_untyped(&mut self, handle: UntypedResourceHandle) -> Box<dyn Any>;
    fn borrow<R: Any>(&self, handle: ResourceHandle<R>) -> &R;
    fn borrow_untyped(&self, handle: UntypedResourceHandle) -> &dyn Any;
}

pub trait ResourceStorer: ResourceInfoProvider {
    fn store<R: Any>(&mut self, handle: ResourceHandle<R>, value: R);
    fn store_untyped(&mut self, handle: UntypedResourceHandle, value: Box<dyn Any>);
}

struct ResourceManager<'a> {
    resources: &'a mut GenMap<ResourceData>,
    type_resources_info: &'a mut HashMap<TypeId, ResourceTypeInfo>,
}

impl<'a> ResourceInfoProvider for ResourceManager<'a> {
    fn resource_from_type<R: GraphResourceId>(&mut self) -> ResourceHandle<R::Resource> {
        let handle = self.type_resources_info.entry(TypeId::of::<R>()).or_insert_with(|| {
            let handle = self.resources.insert(ResourceData {
                label: std::any::type_name::<R>(),
                storage: TypeId::of::<R::Resource>(),
                is_permanent: R::is_permanent(),
                value: None,
            });

            ResourceTypeInfo { handle }
        }).handle;
        ResourceHandle::new(handle)
    }
}

impl ResourceGatherer for ResourceManager<'_> {
    fn consume<R: Any>(&mut self, handle: ResourceHandle<R>) -> R {
        *self.resources.get_mut(handle.inner).expect("Invalid resource handle")
            .value.take().expect("Missing resource")
            .downcast().expect("Stored resource of invalid type")
    }

    fn consume_untyped(&mut self, handle: UntypedResourceHandle) -> Box<dyn Any> {
        self.resources.get_mut(handle.0).expect("Invalid resource handle")
            .value.take().expect("Missing resource")
    }

    fn borrow<R: Any>(&self, handle: ResourceHandle<R>) -> &R {
        self.resources.get(handle.inner).expect("Invalid resource handle")
            .value.as_ref().expect("Missing resource")
            .downcast_ref().expect("Stored resource of invalid type")
    }

    fn borrow_untyped(&self, handle: UntypedResourceHandle) -> &dyn Any {
        self.resources.get(handle.0).expect("Invalid resource handle")
            .value.as_ref().expect("Missing resource")
    }
}

impl ResourceStorer for ResourceManager<'_> {
    fn store<R: Any>(&mut self, handle: ResourceHandle<R>, value: R) {
        todo!()
    }

    fn store_untyped(&mut self, handle: UntypedResourceHandle, value: Box<dyn Any>) {
        todo!()
    }
}

trait GraphNodeWrapperTrait: std::any::Any {
    fn label(&self) -> &str;
    fn run(&mut self, manager: &mut ResourceManager<'_>);
}
sa::assert_obj_safe!(GraphNodeWrapperTrait);
impl<N: GraphNode> GraphNodeWrapperTrait for (N::InputBundle, N, N::OutputBundle) {
    fn label(&self) -> &str {
        self.1.label()
    }
    fn run(&mut self, manager: &mut ResourceManager<'_>) {
        let input = self.0.gather(manager);
        let output = self.1.run(input);
        self.2.store(output, manager);
    }
}

struct NodeData {
    borrows: Vec<UntypedResourceHandle>,
    consumes: Vec<UntypedResourceHandle>,
    outputs: Vec<UntypedResourceHandle>,
    node: Box<dyn GraphNodeWrapperTrait>,
}

impl NodeData {
    fn type_id(&self) -> TypeId {
        self.node.as_ref().type_id()
    }

    fn label(&self) -> &str {
        self.node.label()
    }
}

struct ResourceData {
    label: &'static str,
    storage: TypeId,
    is_permanent: bool,
    value: Option<Box<dyn Any>>,
}

struct ResourceTypeInfo {
    handle: Handle<ResourceData>,
}

pub struct NodeHandle<N> {
    node: PhantomData<fn(N) -> N>,
    inner: genmap::Handle<NodeData>,
}

impl<N> NodeHandle<N> {
    fn new(inner: genmap::Handle<NodeData>) -> Self {
        Self {
            node: PhantomData,
            inner,
        }
    }

    pub fn to_untyped(&self) -> UntypedNodeHandle {
        self.into()
    }
}

impl<N> std::fmt::Debug for NodeHandle<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("NodeHandle")
            .field(&self.inner)
            .finish()
    }
}

impl<N> Clone for NodeHandle<N> {
    fn clone(&self) -> Self {
        Self { node: PhantomData, inner: self.inner.clone() }
    }
}
impl<N> Copy for NodeHandle<N> { }

impl<N> PartialEq for NodeHandle<N> {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node && self.inner == other.inner
    }
}
impl<N> Eq for NodeHandle<N> { }

impl<N> Into<UntypedNodeHandle> for NodeHandle<N> {
    fn into(self) -> UntypedNodeHandle {
        UntypedNodeHandle(self.inner)
    }
}

impl<N> Into<UntypedNodeHandle> for &NodeHandle<N> {
    fn into(self) -> UntypedNodeHandle {
        (*self).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct UntypedNodeHandle(genmap::Handle<NodeData>);

pub struct ResourceHandle<R> {
    node: PhantomData<fn(R) -> R>,
    inner: genmap::Handle<ResourceData>,
}

impl<R> ResourceHandle<R> {
    fn new(inner: genmap::Handle<ResourceData>) -> Self {
        Self {
            node: PhantomData,
            inner,
        }
    }

    pub fn to_untyped(&self) -> UntypedResourceHandle {
        self.into()
    }
}

impl<N> std::fmt::Debug for ResourceHandle<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ResourceHandle")
            .field(&self.inner)
            .finish()
    }
}

impl<N> Clone for ResourceHandle<N> {
    fn clone(&self) -> Self {
        Self { node: PhantomData, inner: self.inner.clone() }
    }
}
impl<N> Copy for ResourceHandle<N> { }

impl<N> PartialEq for ResourceHandle<N> {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node && self.inner == other.inner
    }
}
impl<N> Eq for ResourceHandle<N> { }

impl<N> Into<UntypedResourceHandle> for ResourceHandle<N> {
    fn into(self) -> UntypedResourceHandle {
        UntypedResourceHandle(self.inner)
    }
}

impl<N> Into<UntypedResourceHandle> for &ResourceHandle<N> {
    fn into(self) -> UntypedResourceHandle {
        (*self).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct UntypedResourceHandle(genmap::Handle<ResourceData>);

#[derive(Default)]
pub struct RenderGraph {
    type_name_registry: TypeNameRegistry,
    type_resources_info: HashMap<TypeId, ResourceTypeInfo>,
    inputs: HashSet<UntypedResourceHandle>,
    resources: GenMap<ResourceData>,
    nodes: GenMap<NodeData>,
    explicit_orderings: Vec<(UntypedNodeHandle, UntypedNodeHandle)>,

    compiled: Option<CompiledGraph>,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self::default()
    }

    fn is_resource_permanent(&self, resource: UntypedResourceHandle) -> bool {
        self.resources.get(resource.0).is_some_and(|r| r.is_permanent)
    }

    pub fn resource_from_type<R: GraphResourceId>(&mut self) -> ResourceHandle<R::Resource> {
        ResourceManager {
            resources: &mut self.resources,
            type_resources_info: &mut self.type_resources_info,
        }.resource_from_type::<R>()
    }

    pub fn set_input<R: GraphResourceId>(&mut self, val: R::Resource) {
        let res = self.resource_from_type::<R>();
        self.set_resource_input(res, val);
    }

    pub fn set_resource_input<R: Any>(&mut self, handle: ResourceHandle<R>, value: R) {
        let resource = self.resources.get_mut(handle.inner).expect("Invalid resource handle");
        resource.value = Some(Box::new(value));
    }

    pub fn push_node_complete<N: GraphNode>(&mut self, node: N, input_bundle: N::InputBundle, output_bundle: N::OutputBundle) -> NodeHandle<N> {
        self.type_name_registry.register::<N>();

        let mut manager = ResourceManager {
            resources: &mut self.resources,
            type_resources_info: &mut self.type_resources_info,
        };
        let consumes = input_bundle.list_consumes(&mut manager).into_iter().collect_vec();
        let borrows = input_bundle.list_borrows(&mut manager).into_iter().collect_vec();
        let outputs = output_bundle.list_resources(&mut manager).into_iter().collect_vec();

        let shared = consumes.iter().filter(|o| borrows.contains(o)).collect_vec();
        assert!(shared.is_empty(), "Error pushing Node {} into render graph, the following resources are both consumed and borrowed: {shared:?}", std::any::type_name::<N>());
        let consumed_permanents = consumes.iter().filter(|p| self.resources.get(p.0).is_some_and(|data| data.is_permanent)).collect_vec();
        assert!(consumed_permanents.is_empty(), "Error pushing Node {} into render graph, the following resources cannot be consumed because they are permanent: {consumed_permanents:?}", std::any::type_name::<N>());

        let handle = self.nodes.insert(NodeData {
            consumes,
            borrows,
            outputs,
            node: Box::new((input_bundle, node, output_bundle)),
        });

        self.compiled = None;

        NodeHandle::new(handle)
    }

    pub fn push_node<N: GraphNode>(&mut self, node: N) -> NodeHandle<N>
        where N::InputBundle: Default,
              N::OutputBundle: Default,
    {
        self.push_node_complete(node, Default::default(), Default::default())
    }

    pub fn define_explicit_ordering(&mut self, is_before: impl Into<UntypedNodeHandle>, is_after: impl Into<UntypedNodeHandle>) {
        let is_before = is_before.into();
        let is_after = is_after.into();
        assert!(self.nodes.has(is_before.0) && self.nodes.has(is_after.0), "Given node handles are not valid");
        self.explicit_orderings.push((is_before, is_after));

        self.compiled = None;
    }

    pub fn remove_node<N: GraphNode>(&mut self, handle: NodeHandle<N>) -> Option<N> {
        let node = self.nodes.remove(handle.inner)?;
        self.explicit_orderings.retain(|&(a, b)| a.0 != handle.inner && b.0 != handle.inner);
        self.compiled = None;
        Some(*(node.node as Box<dyn Any>).downcast::<N>().expect("Correct type associated with handle"))
    }

    pub fn prepare_run(&mut self) {
        if self.compiled.is_some() { return; }
        let compiled = CompiledGraph::new(self);
        self.compiled = Some(compiled);
    }

    fn execute(&mut self, steps: &[usize]) {
        for &idx in steps {
            self.nodes.values_mut()[idx].node.run(&mut ResourceManager {
                resources: &mut self.resources,
                type_resources_info: &mut self.type_resources_info,
            });
        }
        for resource in self.resources.iter_mut() {
            if !resource.is_permanent {
                resource.value = None;
            }
        }
    }

    pub fn compute<T: GraphResourceId>(&mut self) -> T {
        // self.prepare_run();
        // let mut compiled = self.compiled.take().unwrap();
        // let result = compiled.compute(self, TypeId::of::<T>());

        // assert!(result.required_inputs.iter().all(|&p| self.resource_store.resources.has_any(p)), "Cannot run, missing inputs!");
        // self.execute(&result.steps);
        // compiled.apply_compute_result(self, &result);
        
        // self.compiled = Some(compiled);

        // *self.resource_store.resources.remove::<T>().unwrap()
    }

    fn write_to_dot(&self, f: &mut impl std::fmt::Write, steps: Option<&[usize]>) -> std::fmt::Result {
        todo!()
        // const INDENT: &'static str = "  ";

        // writeln!(f, "digraph {{")?;
        // let mut names = HashMap::<&'static str, HashMap<TypeId, String>>::new();
        // macro_rules! get_name { ($n: expr, $pref: expr) => {{
        //     let p = names.entry($pref).or_default();
        //     let c = p.len();
        //     p.entry($n).or_insert_with(|| format!("{}{c}", $pref)).clone()
        // }}; }
        // let mut printed_resources = HashSet::<TypeId>::new();
        // macro_rules! write_node { ($name:expr$(,$key:expr=>$val:expr)*) => {{
        //     write!(f, "{INDENT}{} [", $name)?;
        //     $(write!(f, " {}=\"{}\"", stringify!($key), $val)?;)*
        //     writeln!(f, "]")?;
        // }}; }
        // macro_rules! write_resource { ($tid:expr) => {{
        //     let tid = $tid;
        //     if printed_resources.insert(tid) {
        //         let type_name = self.type_name_registry.get_name(tid);
        //         write_node!(get_name!(tid, "R"), shape=>"plain", label=>type_name);
        //     }
        // }}; }

        // for &tid in self.inputs.iter().sorted_by_key(|p| self.type_name_registry.get_name(**p)) {
        //     printed_resources.insert(tid);
        //     let type_name = self.type_name_registry.get_name(tid);
        //     write_node!(get_name!(tid, "R"), shape=>"rectangle", label=>type_name);
        // }
        // for (i, n) in self.nodes.iter().enumerate().sorted_by_key(|(_, n)| n.label()) {
        //     let name = format!("N{i}");
        //     write_node!(name, shape=>"cylinder", label=>format!("{i} {}", n.label()));
        //     for &input in n.inputs {
        //         write_resource!(input);
        //         let input = get_name!(input, "R");
        //         writeln!(f, "{INDENT}{input} -> {name}")?;
        //     }
        //     for &borrowed_input in n.borrowed_inputs {
        //         write_resource!(borrowed_input);
        //         let borrowed_input = get_name!(borrowed_input, "R");
        //         writeln!(f, "{INDENT}{borrowed_input} -> {name} [style=dashed]")?;
        //     }
        //     for &output in n.outputs {
        //         write_resource!(output);
        //         let output = get_name!(output, "R");
        //         writeln!(f, "{INDENT}{name} -> {output}")?;
        //     }
        // }

        // if let Some(steps) = steps {
        //     for [a, b] in steps.iter().copied().array_windows() {
        //         writeln!(f, "{INDENT}N{a} -> N{b} [color=blue]")?;
        //     }
        // }
        
        // write!(f, "}}")?;

        // Ok(())
    }
}
