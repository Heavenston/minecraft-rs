use std::{any::{ Any, TypeId }, collections::{HashMap, HashSet}};
use itertools::Itertools as _;
use typemap::TypeMap;

#[derive(Debug, Clone, Default)]
pub struct TypeNameRegistry {
    content: HashMap<TypeId, &'static str>
}

impl TypeNameRegistry {
    pub fn register<T: Any>(&mut self) {
        self.content.insert(TypeId::of::<T>(), std::any::type_name::<T>());
    }

    pub fn get_name(&self, id: TypeId) -> Option<&'static str> {
        self.content.get(&id).copied()
    }
}

pub trait GraphResourceId: Any {
    type Resource
        where Self: Sized;
    fn new_resource(val: Self::Resource) -> Self
        where Self: Sized;
    fn get_resource(self) -> Self::Resource
        where Self: Sized;
}
typemap::impl_dyn_trait!(GraphResourceId);

#[macro_export]
macro_rules! graph_resource {
    ($(#[$attrs:meta])* $vis:vis struct $name:ident($val_vis:vis $content:ty)) => {
        $(#[$attrs])* $vis struct $name($val_vis $content);
        impl $crate::render_graph::GraphResourceId for $name {
            type Resource = $content
                where Self: Sized;
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
        }
    };
}

pub trait GraphNode: 'static {
    type Inputs;
    type Outputs;

    #[doc(hidden)]
    fn register_resources(registry: &mut TypeNameRegistry);
    #[doc(hidden)]
    fn list_inputs() -> &'static [TypeId];
    #[doc(hidden)]
    fn list_outputs() -> &'static [TypeId];
    #[doc(hidden)]
    fn gather_inputs(store: &mut ResourceStore) -> Self::Inputs;
    #[doc(hidden)]
    fn store_outputs(outputs: Self::Outputs, store: &mut ResourceStore);

    fn run(inputs: Self::Inputs) -> Self::Outputs;
}

#[macro_export]
macro_rules! declare_graph_deps {
    (($($input:ty),*) -> ($($output:ty),*)) => {
        type Inputs = ($(<$input as $crate::render_graph::GraphResourceId>::Resource,)*);
        type Outputs = ($(<$output as $crate::render_graph::GraphResourceId>::Resource,)*);

        fn register_resources(registry: &mut $crate::render_graph::TypeNameRegistry) {
            $(registry.register::<$input>();)*
            $(registry.register::<$output>();)*
        }
        fn list_inputs() -> &'static [::std::any::TypeId] {
            const INPUTS: &'static [::std::any::TypeId] = &[$(::std::any::TypeId::of::<$input>()),*];
            INPUTS
        }
        fn list_outputs() -> &'static [::std::any::TypeId] {
            const OUTPUTS: &'static [::std::any::TypeId] = &[$(::std::any::TypeId::of::<$output>()),*];
            OUTPUTS
        }
        fn gather_inputs(store: &mut $crate::render_graph::ResourceStore) -> Self::Inputs {
            ($(<$input as $crate::render_graph::GraphResourceId>::get_resource(*store.resources.remove::<$input>().expect(std::stringify!(Missing input $input))),)*)
        }
        fn store_outputs(outputs: Self::Outputs, store: &mut $crate::render_graph::ResourceStore) {
            $(store.resources.insert(Box::new(<$output as $crate::render_graph::GraphResourceId>::new_resource(outputs.${index()})));)*
        }
    };
}
pub use declare_graph_deps;

pub struct ResourceStore {
    pub resources: TypeMap<dyn GraphResourceId>,
}

struct NodeData {
    type_id: TypeId,
    run: Box<dyn FnMut(&mut ResourceStore)>,
    inputs: &'static [TypeId],
    outputs: &'static [TypeId],
}

#[derive(Default)]
pub struct RenderGraph {
    type_name_registry: TypeNameRegistry,
    inputs: HashSet<TypeId>,
    input_values: TypeMap<dyn GraphResourceId>,
    nodes: Vec<NodeData>,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn define_input<R: GraphResourceId>(&mut self) {
        self.type_name_registry.register::<R>();
        self.inputs.insert(TypeId::of::<R>());
    }

    pub fn set_input<R: GraphResourceId>(&mut self, val: R::Resource) {
        self.type_name_registry.register::<R>();
        self.inputs.insert(TypeId::of::<R>());
        self.input_values.upsert::<R>(Box::new(R::new_resource(val)));
    }

    pub fn push_node<N: GraphNode>(&mut self) {
        self.type_name_registry.register::<N>();
        N::register_resources(&mut self.type_name_registry);
        self.nodes.push(NodeData {
            type_id: std::any::TypeId::of::<N>(),
            run: Box::new(move |store| {
                let inputs = N::gather_inputs(store);
                let outputs = N::run(inputs);
                N::store_outputs(outputs, store);
            }),
            inputs: N::list_inputs(),
            outputs: N::list_outputs(),
        });
    }

    fn construct_steps(&self) -> Vec<usize> {
        let mut steps = Vec::<usize>::new();
        let mut resources = self.inputs.iter().copied().collect::<HashSet<_>>();
        let mut remaining_nodes = (0usize..self.nodes.len()).collect_vec();

        while !remaining_nodes.is_empty() {
            let mut found_one = false;
            remaining_nodes.retain(|&idx| {
                let all_inputs_available = self.nodes[idx].inputs.iter().copied().all(|input| resources.contains(&input));
                if all_inputs_available {
                    steps.push(idx);
                    for i in self.nodes[idx].inputs { resources.remove(i); }
                    resources.extend(self.nodes[idx].outputs);
                    found_one = true;
                    false
                }
                else {
                    true
                }
            });
            if !found_one {
                panic!("Invalid render graph");
            }
        }

        steps
    }

    pub fn run(&mut self) -> ResourceStore {
        assert!(self.inputs.iter().copied().all(|p| self.input_values.has_any(p)), "Cannot run, missing inputs!");
        let mut resources = ResourceStore {
            resources: std::mem::take(&mut self.input_values),
        };
        for idx in self.construct_steps() {
            (self.nodes[idx].run)(&mut resources);
        }
        resources
    }
}

impl std::fmt::Display for RenderGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const INDENT: &'static str = "  ";

        writeln!(f, "digraph {{")?;
        let mut names = HashMap::<&'static str, HashMap<TypeId, String>>::new();
        macro_rules! get_name { ($n: expr, $pref: expr) => {{
            let p = names.entry($pref).or_default();
            let c = p.len();
            p.entry($n).or_insert_with(|| format!("{}{c}", $pref)).clone()
        }}; }
        let mut printed_resources = HashSet::<TypeId>::new();
        macro_rules! write_node { ($id:expr,$pref:expr,$shape:expr) => {{
            let id = $id;
            printed_resources.insert(id);
            let name = get_name!(id, $pref);
            write!(f, "{INDENT}{name} [shape={}", $shape)?;
            if let Some(type_name) = self.type_name_registry.get_name(id) {
                write!(f, " label=\"{type_name}\"")?;
            }
            writeln!(f, "]")?;
        }}; }
        macro_rules! write_resource { ($t:expr) => {{
            let id = $t;
            if printed_resources.insert(id) {
                write_node!(id, "R", "none");
            }
        }}; }

        for &t in self.inputs.iter() {
            write_node!(t, "R", "rectangle");
        }
        for n in self.nodes.iter() {
            write_node!(n.type_id, "N", "diamond");
            let name = get_name!(n.type_id, "N");
            for &input in n.inputs {
                write_resource!(input);
                let input = get_name!(input, "R");
                writeln!(f, "{INDENT}{input} -> {name}")?;
            }
            for &output in n.outputs {
                write_resource!(output);
                let output = get_name!(output, "R");
                writeln!(f, "{INDENT}{name} -> {output}")?;
            }
        }
        write!(f, "}}")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use super::*;

    graph_resource!(struct R1(u32));
    graph_resource!(#[derive(Debug)] struct R2(u8));

    struct TestNode;
    impl GraphNode for TestNode {
        declare_graph_deps!((R1) -> (R2));
        fn run((val,): (u32,)) -> (u8,) {
            (val as u8,)
        }
    }

    #[test]
    fn test_simple() {
        let mut graph = RenderGraph::new();
        graph.push_node::<TestNode>();
        graph.set_input::<R1>(5);
        let output = graph.run();
        assert_matches!(output.resources.get::<R2>(), Some(&R2(5)));
    }

    graph_resource!(struct Input(String));
    graph_resource!(struct Input1(String));
    graph_resource!(struct Input2(String));
    graph_resource!(struct Reversed(String));
    graph_resource!(struct ReversedHalf(String));
    graph_resource!(struct Half(String));
    graph_resource!(#[derive(Debug, PartialEq)] struct Output(String));

    struct Cloner;
    impl GraphNode for Cloner {
        declare_graph_deps!((Input) -> (Input1, Input2));
        fn run((input,): (String,)) -> (String,String) {
            (input.clone(),input)
        }
    }

    struct Reserver;
    impl GraphNode for Reserver {
        declare_graph_deps!((Input1) -> (Reversed));
        fn run((input,): (String,)) -> (String,) {
            (input.chars().rev().collect(),)
        }
    }

    struct ReversedHalfer;
    impl GraphNode for ReversedHalfer {
        declare_graph_deps!((Reversed) -> (ReversedHalf));
        fn run((input,): (String,)) -> (String,) {
            let count = input.chars().count();
            let half = input.chars().take(count.div_ceil(2)).collect();
            (half,)
        }
    }

    struct InputHalfer;
    impl GraphNode for InputHalfer {
        declare_graph_deps!((Input2) -> (Half));
        fn run((input,): (String,)) -> (String,) {
            let count = input.chars().count();
            let half = input.chars().take(count.div_ceil(2)).collect();
            (half,)
        }
    }

    struct Concat;
    impl GraphNode for Concat {
        declare_graph_deps!((Half, ReversedHalf) -> (Output));
        fn run((a, b): (String, String)) -> (String,) {
            (a.chars().chain(b.chars()).collect(),)
        }
    }

    #[test]
    fn complex_construction() {
        let mut graph = RenderGraph::new();
        graph.push_node::<Cloner>();
        graph.push_node::<Concat>();
        graph.push_node::<ReversedHalfer>();
        graph.push_node::<Reserver>();
        graph.push_node::<InputHalfer>();
        graph.define_input::<Input>();
        assert_eq!(graph.construct_steps(), vec![0,3,4,2,1]);
    }

    #[test]
    fn complex_run() {
        let mut graph = RenderGraph::new();
        graph.push_node::<Cloner>();
        graph.push_node::<Reserver>();
        graph.push_node::<ReversedHalfer>();
        graph.push_node::<InputHalfer>();
        graph.push_node::<Concat>();
        graph.set_input::<Input>(format!("abcdefghi"));
        let output = graph.run();
        assert_eq!(output.resources.get::<Output>(), Some(&Output(format!("abcdeihgfe"))));
    }
}
