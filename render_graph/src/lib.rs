#![feature(macro_metavar_expr)]

mod compiler;

use std::{any::{ Any, TypeId }, collections::{HashMap, HashSet}, marker::PhantomData};
use genmap::GenMap;
use itertools::Itertools as _;
use typemap::TypeMap;

use crate::compiler::CompiledGraph;

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

pub trait ResourceRegisterer {
    fn register<R: GraphResourceId>(&mut self);
}

pub trait GraphNode: 'static {
    type Inputs<'a>;
    type Outputs;

    fn label(&self) -> &str {
        std::any::type_name::<Self>()
    }

    #[doc(hidden)]
    fn register_resources(registerer: &mut impl ResourceRegisterer);
    #[doc(hidden)]
    fn list_inputs() -> &'static [TypeId];
    #[doc(hidden)]
    fn list_borrowed_inputs() -> &'static [TypeId];
    #[doc(hidden)]
    fn list_outputs() -> &'static [TypeId];
    #[doc(hidden)]
    fn gather_inputs(store: &mut ResourceStore) -> Self::Inputs<'_>;
    #[doc(hidden)]
    fn store_outputs(outputs: Self::Outputs, store: &mut ResourceStore);

    fn run(&mut self, inputs: Self::Inputs<'_>) -> Self::Outputs;
}

#[macro_export]
macro_rules! declare_graph_deps {
    (($($input:ty,)*$(ref $borrowed_input:ty,)*) -> ($($output:ty,)*)) => {
        type Inputs<'a> = (
            $(<$input as $crate::GraphResourceId>::Resource,)*
            $(&'a <$borrowed_input as $crate::GraphResourceId>::Resource,)*
        );
        type Outputs = ($(<$output as $crate::GraphResourceId>::Resource,)*);

        #[allow(unused)]
        fn register_resources(registerer: &mut impl $crate::ResourceRegisterer) {
            $(registerer.register::<$input>();)*
            $(registerer.register::<$borrowed_input>();)*
            $(registerer.register::<$output>();)*
        }
        fn list_inputs() -> &'static [::std::any::TypeId] {
            const INPUTS: &'static [::std::any::TypeId] = &[$(::std::any::TypeId::of::<$input>()),*];
            INPUTS
        }
        fn list_borrowed_inputs() -> &'static [::std::any::TypeId] {
            const INPUTS: &'static [::std::any::TypeId] = &[$(::std::any::TypeId::of::<$borrowed_input>()),*];
            INPUTS
        }
        fn list_outputs() -> &'static [::std::any::TypeId] {
            const OUTPUTS: &'static [::std::any::TypeId] = &[$(::std::any::TypeId::of::<$output>()),*];
            OUTPUTS
        }
        #[allow(unused)]
        fn gather_inputs(store: &mut $crate::ResourceStore) -> Self::Inputs<'_> {
            (
                $(<$input as $crate::GraphResourceId>::get_resource(*store.resources.remove::<$input>().expect(std::stringify!(Missing input $input))),)*
                $(<$borrowed_input as $crate::GraphResourceId>::get_resource_ref(store.resources.get::<$borrowed_input>().expect(std::stringify!(Missing input $borrowed_input))),)*
            )
        }
        #[allow(unused)]
        fn store_outputs(outputs: Self::Outputs, store: &mut $crate::ResourceStore) {
            $(store.resources.insert(Box::new(<$output as $crate::GraphResourceId>::new_resource(outputs.${index()})));)*
        }
    };
}

#[derive(Default)]
pub struct ResourceStore {
    pub resources: TypeMap<dyn GraphResourceId>,
}

trait GraphNodeWrapperTrait: std::any::Any {
    fn wrapped_label(&self) -> &str;
    fn wrapped_run(&mut self, store: &mut ResourceStore);
}

impl<N: GraphNode> GraphNodeWrapperTrait for N {
    fn wrapped_label(&self) -> &str {
        self.label()
    }
    fn wrapped_run(&mut self, store: &mut ResourceStore) {
        let inputs = N::gather_inputs(store);
        let outputs = self.run(inputs);
        N::store_outputs(outputs, store);
    }
}

struct NodeData {
    inputs: &'static [TypeId],
    borrowed_inputs: &'static [TypeId],
    outputs: &'static [TypeId],
    node: Box<dyn GraphNodeWrapperTrait>,
}

impl NodeData {
    fn type_id(&self) -> TypeId {
        self.node.as_ref().type_id()
    }

    fn label(&self) -> &str {
        self.node.wrapped_label()
    }
}

struct ResourceTypeInfo {
    is_permanent: bool,
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

#[derive(Default)]
pub struct RenderGraph {
    type_name_registry: TypeNameRegistry,
    resources_infos: HashMap<TypeId, ResourceTypeInfo>,
    inputs: HashSet<TypeId>,
    resource_store: ResourceStore,
    nodes: GenMap<NodeData>,
    explicit_orderings: Vec<(UntypedNodeHandle, UntypedNodeHandle)>,

    compiled: Option<CompiledGraph>,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self::default()
    }

    fn is_resource_permanent(&self, tid: &TypeId) -> bool {
        self.resources_infos.get(tid).is_some_and(|p| p.is_permanent)
    }

    pub fn define_input<R: GraphResourceId>(&mut self) {
        self.type_name_registry.register::<R>();
        self.inputs.insert(TypeId::of::<R>());

        self.compiled = None;
    }

    pub fn set_input<R: GraphResourceId>(&mut self, val: R::Resource) {
        self.type_name_registry.register::<R>();
        self.resource_store.resources.upsert::<R>(Box::new(R::new_resource(val)));
        if self.inputs.insert(TypeId::of::<R>()) {
            self.compiled = None;
        }
        else if R::is_permanent() && let Some(mut compiled) = self.compiled.take() {
            compiled.mark_resource_dirty(self, TypeId::of::<R>());
            self.compiled = Some(compiled);
        }
    }

    pub fn push_node<N: GraphNode>(&mut self, node: N) -> NodeHandle<N> {
        self.type_name_registry.register::<N>();
        struct Registerer<'a> {
            type_name_registry: &'a mut TypeNameRegistry,
            resources: &'a mut HashMap<TypeId, ResourceTypeInfo>,
        }
        impl ResourceRegisterer for Registerer<'_> {
            fn register<R: GraphResourceId>(&mut self) {
                self.type_name_registry.register::<R>();
                self.resources.entry(TypeId::of::<R>()).or_insert_with(|| {
                    ResourceTypeInfo {
                        is_permanent: R::is_permanent(),
                    }
                });
            }
        }
        N::register_resources(&mut Registerer {
            type_name_registry: &mut self.type_name_registry,
            resources: &mut self.resources_infos,
        });

        let inputs = N::list_inputs();
        let borrowed_inputs = N::list_borrowed_inputs();
        let shared = inputs.iter().filter(|o| borrowed_inputs.contains(o)).collect_vec();
        assert!(shared.is_empty(), "Error pushing Node {} into render graph, the following resources are both consumed and borrowed: {shared:?}", std::any::type_name::<N>());
        let consumed_permanents = inputs.iter().filter(|p| self.resources_infos.get(p).is_some_and(|data| data.is_permanent)).collect_vec();
        assert!(consumed_permanents.is_empty(), "Error pushing Node {} into render graph, the following resources cannot be consumed because they are permanent: {consumed_permanents:?}", std::any::type_name::<N>());

        let handle = self.nodes.insert(NodeData {
            inputs,
            borrowed_inputs,
            outputs: N::list_outputs(),
            node: Box::new(node),
        });

        self.compiled = None;

        NodeHandle::new(handle)
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
        for idx in steps.iter().copied() {
            self.nodes.values_mut()[idx].node.wrapped_run(&mut self.resource_store);
        }
        for (&tid, t) in &self.resources_infos {
            if !t.is_permanent {
                self.resource_store.resources.remove_dyn(tid);
            }
        }
    }

    pub fn compute<T: GraphResourceId>(&mut self) -> T {
        self.prepare_run();
        let mut compiled = self.compiled.take().unwrap();
        let result = compiled.compute(self, TypeId::of::<T>());

        assert!(result.required_inputs.iter().all(|&p| self.resource_store.resources.has_any(p)), "Cannot run, missing inputs!");
        self.execute(&result.steps);
        compiled.apply_compute_result(self, &result);
        
        self.compiled = Some(compiled);

        *self.resource_store.resources.remove::<T>().unwrap()
    }

    fn write_to_dot(&self, f: &mut impl std::fmt::Write, steps: Option<&[usize]>) -> std::fmt::Result {
        const INDENT: &'static str = "  ";

        writeln!(f, "digraph {{")?;
        let mut names = HashMap::<&'static str, HashMap<TypeId, String>>::new();
        macro_rules! get_name { ($n: expr, $pref: expr) => {{
            let p = names.entry($pref).or_default();
            let c = p.len();
            p.entry($n).or_insert_with(|| format!("{}{c}", $pref)).clone()
        }}; }
        let mut printed_resources = HashSet::<TypeId>::new();
        macro_rules! write_node { ($name:expr$(,$key:expr=>$val:expr)*) => {{
            write!(f, "{INDENT}{} [", $name)?;
            $(write!(f, " {}=\"{}\"", stringify!($key), $val)?;)*
            writeln!(f, "]")?;
        }}; }
        macro_rules! write_resource { ($tid:expr) => {{
            let tid = $tid;
            if printed_resources.insert(tid) {
                let type_name = self.type_name_registry.get_name(tid);
                write_node!(get_name!(tid, "R"), shape=>"plain", label=>type_name);
            }
        }}; }

        for &tid in self.inputs.iter().sorted_by_key(|p| self.type_name_registry.get_name(**p)) {
            printed_resources.insert(tid);
            let type_name = self.type_name_registry.get_name(tid);
            write_node!(get_name!(tid, "R"), shape=>"rectangle", label=>type_name);
        }
        for (i, n) in self.nodes.iter().enumerate().sorted_by_key(|(_, n)| n.label()) {
            let name = format!("N{i}");
            write_node!(name, shape=>"cylinder", label=>format!("{i} {}", n.label()));
            for &input in n.inputs {
                write_resource!(input);
                let input = get_name!(input, "R");
                writeln!(f, "{INDENT}{input} -> {name}")?;
            }
            for &borrowed_input in n.borrowed_inputs {
                write_resource!(borrowed_input);
                let borrowed_input = get_name!(borrowed_input, "R");
                writeln!(f, "{INDENT}{borrowed_input} -> {name} [style=dashed]")?;
            }
            for &output in n.outputs {
                write_resource!(output);
                let output = get_name!(output, "R");
                writeln!(f, "{INDENT}{name} -> {output}")?;
            }
        }

        if let Some(steps) = steps {
            for [a, b] in steps.iter().copied().array_windows() {
                writeln!(f, "{INDENT}N{a} -> N{b} [color=blue]")?;
            }
        }
        
        write!(f, "}}")?;

        Ok(())
    }
}
