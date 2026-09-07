#![allow(dead_code, reason = "Refactor to using node handles not done yet")]

mod bundles;
pub use bundles::*;
mod node;
use indexmap::MapIndex as _;
pub use node::*;
mod compiler;
#[cfg(test)]
mod tests;
mod label;
pub use label::Label;

use std::{ any::{ Any, TypeId }, borrow::Cow, collections::{HashMap, HashSet, hash_map}, marker::PhantomData, ops::BitOr };
use static_assertions as sa;
use genmap::{AssumeAlive, GenMap, Handle};
use itertools::Itertools as _;

pub use render_graph_macros::{ input_bundle, output_bundle, node_helper };

use crate::compiler::CompiledGraph;

fn get_simplified_labels<K: std::hash::Hash + Eq + Clone>(values: &HashMap<K, Label>) -> HashMap<K, String> {
    let mut node_labels = HashMap::<K, String>::new();
    let mut todo_stack: Vec<K> = values.keys().cloned().collect();
    let mut taken_node_labels = HashMap::<String, Vec<K>>::new();
    while let Some(handle) = todo_stack.pop() {
        let full_name = match &values[&handle] {
            &Label::TypeName(tn) => tn,
            Label::Other(cow) => {
                node_labels.insert(handle, cow.to_string());
                continue;
            },
        };
        let possible_names = [
            full_name.split("::").last().unwrap(),
            full_name,
        ];
        let possible_names = if full_name.contains(['<', '>']) {
            &possible_names[1..]
        } else {
            &possible_names[..]
        };
        for name in possible_names {
            let takeners = taken_node_labels.entry(name.to_string());
            match takeners {
                hash_map::Entry::Occupied(mut entry) => {
                    for t in entry.get_mut().drain(..) {
                        node_labels.remove(&t);
                        todo_stack.push(t);
                    }
                },
                hash_map::Entry::Vacant(entry) => {
                    entry.insert(vec![handle.clone()]);
                    node_labels.insert(handle, name.to_string());
                    break;
                },
            }
        }
    }
    node_labels
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResourceConfig {
    pub permanent: bool,
    pub unordered: bool,
}

impl ResourceConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn permanent() -> Self {
        Self {
            permanent: true,
            ..Default::default()
        }
    }

    pub fn unordered() -> Self {
        Self {
            unordered: true,
            ..Default::default()
        }
    }
}

impl BitOr for ResourceConfig {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            permanent: self.permanent | rhs.permanent,
            unordered: self.unordered | rhs.unordered,
        }
    }
}

pub trait GraphResourceId: Any {
    type Resource
        where Self: Sized;
    fn config() -> ResourceConfig
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
    (@permanence; permanent$($rest:tt)*) => { $crate::ResourceConfig::permanent() };
    (@permanence; $ident:ident$($rest:tt)*) => { $crate::graph_resource!(@permanence $($rest)*) };
    (@permanence) => { $crate::ResourceConfig::default() };

    (@unordered; unordered$($rest:tt)*) => { $crate::ResourceConfig::unordered() };
    (@unordered; $ident:ident$($rest:tt)*) => { $crate::graph_resource!(@unordered $($rest)*) };
    (@unordered) => { $crate::ResourceConfig::default() };

    ($(#[$attrs:meta])* $vis:vis struct $name:ident($val_vis:vis $content:ty)$($parameters:tt)*) => {
        $(#[$attrs])* $vis struct $name($val_vis $content);
        impl $crate::GraphResourceId for $name {
            type Resource = $content
                where Self: Sized;
            fn config() -> $crate::ResourceConfig
                where Self: Sized,
            {
                $crate::graph_resource!(@permanence $($parameters)*) |
                $crate::graph_resource!(@unordered $($parameters)*)
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
                label: Label::TypeName(std::any::type_name::<R>()),
                storage: TypeId::of::<R::Resource>(),
                config: R::config(),
                value: Box::new(None::<R::Resource>),
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
        self.resources.get_mut(handle.inner).expect("valid resource handle")
            .get_mut().take().expect("resource present")
    }

    fn consume_untyped(&mut self, handle: UntypedResourceHandle) -> Box<dyn Any> {
        self.resources.get_mut(handle.0).expect("valid resource handle")
            .value.dyn_take().expect("resource present")
    }

    fn borrow<R: Any>(&self, handle: ResourceHandle<R>) -> &R {
        self.resources.get(handle.inner).expect("valid resource handle")
            .get().as_ref().expect("resource present")
    }

    fn borrow_untyped(&self, handle: UntypedResourceHandle) -> &dyn Any {
        self.resources.get(handle.0).expect("valid resource handle")
            .value.dyn_as_ref().expect("resource present")
    }
}

impl ResourceStorer for ResourceManager<'_> {
    fn store<R: Any>(&mut self, handle: ResourceHandle<R>, value: R) {
        *self.resources.get_mut(handle.inner).expect("valid resource handle").get_mut() = Some(value);
    }

    fn store_untyped(&mut self, handle: UntypedResourceHandle, value: Box<dyn Any>) {
        self.resources.get_mut(handle.0).expect("valid resource handle")
            .value.dyn_insert(value);
    }
}

trait GraphNodeWrapperTrait: std::any::Any {
    fn label(&self) -> Label<'_>;
    fn run(&mut self, manager: &mut ResourceManager<'_>);
}
sa::assert_obj_safe!(GraphNodeWrapperTrait);
impl<N: GraphNode> GraphNodeWrapperTrait for (N::InputBundle, N, N::OutputBundle) {
    fn label(&self) -> Label<'_> {
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

    fn label(&self) -> Label<'_> {
        self.node.label()
    }
}

trait DynOption: Any {
    fn dyn_is_some(&self) -> bool;
    fn dyn_insert(&mut self, other: Box<dyn Any>);
    fn dyn_take(&mut self) -> Option<Box<dyn Any>>;
    fn dyn_as_ref(&self) -> Option<&dyn Any>;
    fn dyn_clear(&mut self);
}
impl<T: Any> DynOption for Option<T> {
    fn dyn_is_some(&self) -> bool {
        self.is_some()
    }

    fn dyn_insert(&mut self, other: Box<dyn Any>) {
        *self = Some(*other.downcast().expect("correct type"));
    }

    fn dyn_take(&mut self) -> Option<Box<dyn Any>> {
        self.take().map(|val| Box::new(val) as Box<dyn Any>)
    }

    fn dyn_as_ref(&self) -> Option<&dyn Any> {
        self.as_ref().map(|p| p as &dyn Any)
    }

    fn dyn_clear(&mut self) {
        *self = None;
    }
}

struct ResourceData {
    label: Label<'static>,
    storage: TypeId,
    config: ResourceConfig,
    value: Box<dyn DynOption>,
}

impl ResourceData {
    fn get<T: 'static>(&self) -> Option<&T> {
        (self.value.as_ref() as &dyn Any).downcast_ref::<Option<T>>().expect("resource value slot has correct type").as_ref()
    }

    fn get_mut<T: 'static>(&mut self) -> &mut Option<T> {
        assert_eq!(self.value.as_ref().type_id(), TypeId::of::<Option<T>>());
        (self.value.as_mut() as &mut dyn Any).downcast_mut::<Option<T>>().expect("resource value slot has correct type")
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
        self.resources.get(resource.0).is_some_and(|r| r.config.permanent)
    }

    pub fn resource_from_type<R: GraphResourceId>(&mut self) -> ResourceHandle<R::Resource> {
        ResourceManager {
            resources: &mut self.resources,
            type_resources_info: &mut self.type_resources_info,
        }.resource_from_type::<R>()
    }

    pub fn create_resource<S: Any>(&mut self, label: Cow<'static, str>, config: ResourceConfig) -> ResourceHandle<S> {
        let handle = self.resources.insert(ResourceData {
            label: Label::Other(label),
            storage: TypeId::of::<S>(),
            config,
            value: Box::new(None::<S>),
        });
        ResourceHandle { node: PhantomData, inner: handle }
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
        self.set_resource_input::<R::Resource>(res, val);
    }

    pub fn set_resource_input<R: Any>(&mut self, handle: ResourceHandle<R>, value: R) {
        *self.resources.get_mut(handle.inner).expect("Invalid resource handle").get_mut() = Some(value);
        if self.inputs.insert(handle.into()) {
            self.compiled = None;
        }
        if self.is_resource_permanent(handle.to_untyped()) && let Some(mut compiled) = self.compiled.take() {
            compiled.mark_resource_dirty(self, handle.into());
            self.compiled = Some(compiled);
        }
    }

    pub fn set_resource_input_untyped(&mut self, handle: UntypedResourceHandle, value: Box<dyn Any>) {
        self.resources.get_mut(handle.0).expect("Invalid resource handle").value.dyn_insert(value);
        if self.inputs.insert(handle.into()) {
            self.compiled = None;
        }
        if self.is_resource_permanent(handle) && let Some(mut compiled) = self.compiled.take() {
            compiled.mark_resource_dirty(self, handle.into());
            self.compiled = Some(compiled);
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
        let consumed_permanents = consumes.iter().filter(|p| self.resources.with(AssumeAlive(p.0)).get().config.permanent).collect_vec();
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
        if let Some(path) = option_env!("DEBUG_GRAPH_PRECOMPUTE_PATH") {
            let mut str = String::new();
            self.write_to_dot(&mut str, None).unwrap();
            std::fs::write(path, str).unwrap();
            tracing::debug!(output = path, "Stored prepare_run() dot graph");
        }

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
            if !resource.config.permanent {
                resource.value.dyn_clear();
            }
        }
    }

    fn compute_inner(&mut self, resource: UntypedResourceHandle) {
        assert!(self.resources.has(resource.0), "Invalid resource handle");

        let mut compiled = self.compiled.take().unwrap_or_else(|| CompiledGraph::new(self));
        let result = compiled.compute(self, resource.into());

        assert!(result.required_inputs.iter().all(|&p| self.resources.get(AssumeAlive(p.0)).value.dyn_is_some()), "Cannot run, missing inputs!");
        self.execute(&result.steps);
        compiled.apply_compute_result(self, &result);
        
        self.compiled = Some(compiled);
    }

    pub fn compute<T: GraphResourceId>(&mut self) -> T::Resource {
        let resource = self.resource_from_type::<T>();
        self.compute_resource(resource)
    }

    pub fn compute_resource<T: 'static>(&mut self, resource: ResourceHandle<T>) -> T {
        self.compute_inner(resource.to_untyped());
        self.resources.get_mut(resource.inner).expect("valid resource handle").get_mut().take().expect("value was created during compute")
    }

    pub fn compute_untyped(&mut self, resource: UntypedResourceHandle) -> Box<dyn Any> {
        self.compute_inner(resource);
        self.resources.get_mut(resource.0).expect("valid resource handle").value.dyn_take().expect("value was created during compute")
    }

    fn get_simplified_node_labels(&self) -> HashMap<UncheckedNodeHandle, String> {
        get_simplified_labels(&self.nodes.enumerated().map(|(sparse_idx,_,n)| (UncheckedNodeHandle(sparse_idx), n.label().to_static())).collect())
    }

    fn get_simplified_resource_labels(&self) -> HashMap<UncheckedResourceHandle, String> {
        let mut simplified = get_simplified_labels(&self.resources.enumerated().map(|(sparse_idx,_,n)| (UncheckedResourceHandle(sparse_idx), n.label.to_static())).collect());
        #[expect(clippy::iter_over_hash_type, reason = "Iteration order is not observable")]
        for (k, v) in &mut simplified {
            let cfg = &self.resources.with(AssumeAlive(k.0)).get().config;
            if cfg.unordered {
                v.push('~');
            }
            if cfg.permanent {
                v.push('*');
            }
        }
        simplified
    }

    fn write_to_dot(&self, f: &mut impl std::fmt::Write, steps: Option<&[UncheckedNodeHandle]>) -> std::fmt::Result {
        const INDENT: &str = "  ";

        let node_labels = self.get_simplified_node_labels();
        let resource_labels = self.get_simplified_resource_labels();

        macro_rules! rn {
            ($t:expr) => { &resource_labels[&$t] };
        }

        writeln!(f, "digraph {{")?;
        let mut names = HashMap::<&'static str, HashMap<UncheckedResourceHandle, String>>::new();
        macro_rules! get_name { ($n: expr, $pref: expr) => {{
            let p = names.entry($pref).or_default();
            let c = p.len();
            p.entry($n).or_insert_with(|| format!("{}{c}", $pref)).clone()
        }}; }
        let mut printed_resources = HashSet::<UncheckedResourceHandle>::new();
        macro_rules! write_node { ($name:expr$(,$key:expr=>$val:expr)*) => {{
            write!(f, "{INDENT}{} [", $name)?;
            $(write!(f, " {}=\"{}\"", stringify!($key), $val)?;)*
            writeln!(f, "]")?;
        }}; }
        macro_rules! write_resource { ($resource:expr) => {{
            let resource = $resource;
            if printed_resources.insert(resource) {
                write_node!(get_name!(resource, "R"), shape=>"plain", label=>rn!($resource));
            }
        }}; }

        for &resource in self.inputs.iter().sorted_by_key(|resource| rn!(resource)) {
            printed_resources.insert(resource);
            write_node!(get_name!(resource, "R"), shape=>"rectangle", label=>rn!(resource));
        }
        for (i,_,n) in self.nodes.enumerated() {
            let i = UncheckedNodeHandle(i);
            let name = format!("N{}", i.0.as_usize());
            write_node!(name, shape=>"cylinder", label=>&node_labels[&i]);
            for &resource in &n.consumes {
                write_resource!(resource);
                let resource_node = get_name!(resource, "R");
                writeln!(f, "{INDENT}{resource_node} -> {name}")?;
            }
            for &resource in &n.borrows {
                write_resource!(resource);
                let resource_node = get_name!(resource, "R");
                writeln!(f, "{INDENT}{resource_node} -> {name} [style=dashed]")?;
            }
            for &resource in &n.outputs {
                write_resource!(resource);
                let resource_node = get_name!(resource, "R");
                writeln!(f, "{INDENT}{name} -> {resource_node}")?;
            }
        }

        if let Some(steps) = steps {
            for [a, b] in steps.iter().copied().array_windows() {
                writeln!(f, "{INDENT}N{} -> N{} [color=blue]", a.0.as_usize(), b.0.as_usize())?;
            }
        }
        
        write!(f, "}}")?;

        Ok(())
    }
}
