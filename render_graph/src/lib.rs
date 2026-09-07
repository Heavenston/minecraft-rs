#![allow(dead_code, reason = "Refactor to using node handles not done yet")]

mod bundles;
pub use bundles::*;
mod node;
pub use node::*;
mod compiler;
#[cfg(test)]
mod tests;

use std::{any::{ Any, TypeId }, collections::{HashMap, HashSet}, marker::PhantomData, ops::Deref as _};
use static_assertions as sa;
use genmap::{AssumeAlive, GenMap, Handle};
use itertools::Itertools as _;

pub use render_graph_macros::{ input_bundle, output_bundle, node_helper };

use crate::compiler::CompiledGraph;

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

impl ResourceInfoProvider for ResourceManager<'_> {
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
        ResourceHandle {
            node: PhantomData,
            inner: handle,
        }
    }
}

impl ResourceGatherer for ResourceManager<'_> {
    fn consume<R: Any>(&mut self, handle: ResourceHandle<R>) -> R {
        *self.consume_untyped(handle.to_untyped())
            .downcast().expect("Stored resource of invalid type")
    }

    fn consume_untyped(&mut self, handle: UntypedResourceHandle) -> Box<dyn Any> {
        self.resources.get_mut(handle.0).expect("Invalid resource handle")
            .value.take().expect("Missing resource")
    }

    fn borrow<R: Any>(&self, handle: ResourceHandle<R>) -> &R {
        self.borrow_untyped(handle.to_untyped())
            .downcast_ref().expect("Stored resource of invalid type")
    }

    fn borrow_untyped(&self, handle: UntypedResourceHandle) -> &dyn Any {
        self.resources.get(handle.0).expect("Invalid resource handle")
            .value.as_ref().expect("Missing resource")
            .deref()
    }
}

impl ResourceStorer for ResourceManager<'_> {
    fn store<R: Any>(&mut self, handle: ResourceHandle<R>, value: R) {
        self.store_untyped(handle.to_untyped(), Box::new(value));
    }

    fn store_untyped(&mut self, handle: UntypedResourceHandle, value: Box<dyn Any>) {
        self.resources.get_mut(handle.0).expect("Invalid resource handle").checked_insert(value);
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
    borrows: Vec<UncheckedResourceHandle>,
    consumes: Vec<UncheckedResourceHandle>,
    outputs: Vec<UncheckedResourceHandle>,
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

impl ResourceData {
    fn checked_insert(&mut self, value: Box<dyn Any>) {
        debug_assert_eq!(self.storage, value.deref().type_id(), "given value type does not match expected storage type of resource");
        self.value = Some(value);
    }
}

struct ResourceTypeInfo {
    handle: Handle<ResourceData>,
}

pub struct NodeHandle<N> {
    node: PhantomData<fn(N) -> N>,
    inner: genmap::Handle<NodeData>,
}

impl<N> NodeHandle<N> {
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
        *self
    }
}
impl<N> Copy for NodeHandle<N> { }

impl<N> PartialEq for NodeHandle<N> {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node && self.inner == other.inner
    }
}
impl<N> Eq for NodeHandle<N> { }

impl<N> std::hash::Hash for NodeHandle<N> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct UntypedNodeHandle(genmap::Handle<NodeData>);

impl<N> From<NodeHandle<N>> for UntypedNodeHandle {
    fn from(val: NodeHandle<N>) -> Self {
        Self(val.inner)
    }
}

impl<N> From<&NodeHandle<N>> for UntypedNodeHandle {
    fn from(val: &NodeHandle<N>) -> Self {
        (*val).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
struct UncheckedNodeHandle(genmap::SparseIdx);

impl From<UntypedNodeHandle> for UncheckedNodeHandle {
    fn from(value: UntypedNodeHandle) -> Self {
        Self(value.0.sparse_index())
    }
}

pub struct ResourceHandle<R> {
    node: PhantomData<fn(R) -> R>,
    inner: genmap::Handle<ResourceData>,
}

impl<R> ResourceHandle<R> {
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
        *self
    }
}
impl<N> Copy for ResourceHandle<N> { }

impl<N> PartialEq for ResourceHandle<N> {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node && self.inner == other.inner
    }
}
impl<N> Eq for ResourceHandle<N> { }

impl<N> std::hash::Hash for ResourceHandle<N> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct UntypedResourceHandle(genmap::Handle<ResourceData>);

impl<N> From<ResourceHandle<N>> for UntypedResourceHandle {
    fn from(val: ResourceHandle<N>) -> Self {
        Self(val.inner)
    }
}

impl<N> From<&ResourceHandle<N>> for UntypedResourceHandle {
    fn from(val: &ResourceHandle<N>) -> Self {
        (*val).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
struct UncheckedResourceHandle(genmap::SparseIdx);

impl From<UntypedResourceHandle> for UncheckedResourceHandle {
    fn from(value: UntypedResourceHandle) -> Self {
        Self(value.0.sparse_index())
    }
}

impl<T> From<ResourceHandle<T>> for UncheckedResourceHandle {
    fn from(value: ResourceHandle<T>) -> Self {
        Self(value.inner.sparse_index())
    }
}

#[derive(Default)]
pub struct RenderGraph {
    type_resources_info: HashMap<TypeId, ResourceTypeInfo>,
    inputs: HashSet<UncheckedResourceHandle>,
    resources: GenMap<ResourceData>,
    nodes: GenMap<NodeData>,
    explicit_orderings: Vec<(UncheckedNodeHandle, UncheckedNodeHandle)>,

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

    pub fn create_resource<S: Any>(&mut self, is_permanent: bool) -> ResourceHandle<S> {
        ResourceHandle { node: PhantomData, inner: self.create_resource_untyped(std::any::type_name::<S>(), TypeId::of::<S>(), is_permanent).0 }
    }

    pub fn create_resource_untyped(&mut self, label: &'static str, storage: TypeId, is_permanent: bool) -> UntypedResourceHandle {
        let handle = self.resources.insert(ResourceData {
            label,
            storage,
            is_permanent,
            value: None,
        });
        UntypedResourceHandle(handle)
    }

    pub fn define_input<R: GraphResourceId>(&mut self) {
        let res = self.resource_from_type::<R>();
        self.define_resource_input(res.to_untyped());
    }

    pub fn define_resource_input(&mut self, handle: UntypedResourceHandle) {
        assert!(self.resources.has(handle.0), "Invalid resource handle");
        self.inputs.insert(handle.into());
    }

    pub fn set_input<R: GraphResourceId>(&mut self, val: R::Resource) {
        let res = self.resource_from_type::<R>();
        self.set_resource_input(res, val);
    }

    pub fn set_resource_input<R: Any>(&mut self, handle: ResourceHandle<R>, value: R) {
        self.set_resource_input_untyped(handle.to_untyped(), Box::new(value));
    }

    pub fn set_resource_input_untyped(&mut self, handle: UntypedResourceHandle, value: Box<dyn Any>) {
        self.resources.get_mut(handle.0).expect("Invalid resource handle").checked_insert(value);
        if self.inputs.insert(handle.into()) {
            self.compiled = None;
        }
    }

    pub fn push_node_complete<N: GraphNode>(&mut self, node: N, input_bundle: N::InputBundle, output_bundle: N::OutputBundle) -> NodeHandle<N> {
        let mut manager = ResourceManager {
            resources: &mut self.resources,
            type_resources_info: &mut self.type_resources_info,
        };
        let consumes = input_bundle.list_consumes(&mut manager).into_iter().collect_vec();
        let borrows = input_bundle.list_borrows(&mut manager).into_iter().collect_vec();
        let outputs = output_bundle.list_resources(&mut manager).into_iter().collect_vec();
        let mapper = |handle: UntypedResourceHandle| -> UncheckedResourceHandle {
            assert!(self.resources.has(handle.0), "Resource handle not valid");
            UncheckedResourceHandle(handle.0.sparse_index())
        };
        let consumes = consumes.into_iter().map(mapper).collect_vec();
        let borrows = borrows.into_iter().map(mapper).collect_vec();
        let outputs = outputs.into_iter().map(mapper).collect_vec();

        let shared = consumes.iter().filter(|o| borrows.contains(o)).collect_vec();
        assert!(shared.is_empty(), "Error pushing Node {} into render graph, the following resources are both consumed and borrowed: {shared:?}", std::any::type_name::<N>());
        let consumed_permanents = consumes.iter().filter(|p| self.resources.with(AssumeAlive(p.0)).get().is_permanent).collect_vec();
        assert!(consumed_permanents.is_empty(), "Error pushing Node {} into render graph, the following resources cannot be consumed because they are permanent: {consumed_permanents:?}", std::any::type_name::<N>());

        let handle = self.nodes.insert(NodeData {
            consumes,
            borrows,
            outputs,
            node: Box::new((input_bundle, node, output_bundle)),
        });

        self.compiled = None;

        NodeHandle { node: PhantomData, inner: handle }
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
        self.explicit_orderings.push((UncheckedNodeHandle(is_before.0.sparse_index()), UncheckedNodeHandle(is_after.0.sparse_index())));

        self.compiled = None;
    }

    pub fn remove_node<N: GraphNode>(&mut self, handle: NodeHandle<N>) -> Option<N> {
        Some(*self.remove_node_untyped(handle.to_untyped())?.downcast::<N>().expect("Correct type associated with handle"))
    }

    pub fn remove_node_untyped(&mut self, handle: UntypedNodeHandle) -> Option<Box<dyn Any>> {
        let node = self.nodes.remove(handle.0)?;
        self.explicit_orderings.retain(|&(a, b)| a.0 != handle.0.sparse_index() && b.0 != handle.0.sparse_index());
        self.compiled = None;
        Some(node.node as Box<dyn Any>)
    }

    pub fn prepare_run(&mut self) {
        if self.compiled.is_some() { return; }
        let compiled = CompiledGraph::new(self);
        self.compiled = Some(compiled);
    }

    fn execute(&mut self, steps: &[UncheckedNodeHandle]) {
        for node in steps {
            self.nodes.get_mut(AssumeAlive(node.0)).node.run(&mut ResourceManager {
                resources: &mut self.resources,
                type_resources_info: &mut self.type_resources_info,
            });
        }
        for resource in &mut self.resources {
            if !resource.is_permanent {
                resource.value = None;
            }
        }
    }

    pub fn compute<T: GraphResourceId>(&mut self) -> T::Resource {
        let resource = self.resource_from_type::<T>();
        self.compute_resource(resource)
    }

    pub fn compute_resource<T: 'static>(&mut self, resource: ResourceHandle<T>) -> T {
        *self.compute_untyped(resource.to_untyped()).downcast().expect("correct type inside storage")
    }

    pub fn compute_untyped(&mut self, resource: UntypedResourceHandle) -> Box<dyn Any> {
        assert!(self.resources.has(resource.0), "Invalid resource handle");

        let mut compiled = self.compiled.take().unwrap_or_else(|| CompiledGraph::new(self));
        let result = compiled.compute(self, resource.into());

        assert!(result.required_inputs.iter().all(|&p| self.resources.get(AssumeAlive(p.0)).value.is_some()), "Cannot run, missing inputs!");
        self.execute(&result.steps);
        compiled.apply_compute_result(self, &result);
        
        self.compiled = Some(compiled);

        self.resources.get_mut(resource.0).expect("valid resource handle").value.take().expect("value was created during compute")
    }

    fn write_to_dot(&self, f: &mut impl std::fmt::Write, steps: Option<&[UncheckedNodeHandle]>) -> std::fmt::Result {
        let _ = &mut *f;
        let _ = steps;
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
