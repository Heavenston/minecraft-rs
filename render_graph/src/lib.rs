#![feature(macro_metavar_expr)]

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
    fn get_resource_ref(&self) -> &Self::Resource
        where Self: Sized;
}
typemap::impl_dyn_trait!(GraphResourceId);

#[macro_export]
macro_rules! graph_resource {
    ($(#[$attrs:meta])* $vis:vis struct $name:ident($val_vis:vis $content:ty)) => {
        $(#[$attrs])* $vis struct $name($val_vis $content);
        impl $crate::GraphResourceId for $name {
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
            fn get_resource_ref(&self) -> &Self::Resource
                where Self: Sized
            {
                &self.0
            }
        }
    };
}

pub trait GraphNode: 'static {
    type Inputs<'a>;
    type Outputs;

    #[doc(hidden)]
    fn register_resources(registry: &mut TypeNameRegistry);
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

    fn run(inputs: Self::Inputs<'_>) -> Self::Outputs;
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
        fn register_resources(registry: &mut $crate::TypeNameRegistry) {
            $(registry.register::<$input>();)*
            $(registry.register::<$output>();)*
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

pub struct ResourceStore {
    pub resources: TypeMap<dyn GraphResourceId>,
}

struct NodeData {
    type_id: TypeId,
    run: Box<dyn FnMut(&mut ResourceStore)>,
    inputs: &'static [TypeId],
    borrowed_inputs: &'static [TypeId],
    outputs: &'static [TypeId],
}

#[derive(Debug, thiserror::Error)]
enum GraphValidationError {
    #[error("Resource '{resource_type_name}' consumed by multiple nodes: {consumers:?}")]
    ResourceConsumedMultipleTimes {
        resource_type_name: &'static str,
        consumers: Vec<&'static str>,
    },
    #[error("Resource '{resource_type_name}' emitted by multiple nodes: {emitters:?}")]
    ResourceEmittedMultipleTimes {
        resource_type_name: &'static str,
        emitters: Vec<&'static str>,
    },
    #[error("Resource '{resource_type_name}' is consumed by {consumers:?} but never emitted")]
    ResourceLackEmitter {
        resource_type_name: &'static str,
        consumers: Vec<&'static str>,
    },
    #[error("Could not found satisfying graph ordering")]
    UnsatisfiableOrdering { },
}

type GraphEvaluationSteps = Box<[Box<[usize]>]>;

#[derive(Default)]
pub struct RenderGraph {
    type_name_registry: TypeNameRegistry,
    inputs: HashSet<TypeId>,
    input_values: TypeMap<dyn GraphResourceId>,
    nodes: Vec<NodeData>,

    steps: Option<GraphEvaluationSteps>,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn define_input<R: GraphResourceId>(&mut self) {
        self.type_name_registry.register::<R>();
        self.inputs.insert(TypeId::of::<R>());

        self.steps = None;
    }

    pub fn set_input<R: GraphResourceId>(&mut self, val: R::Resource) {
        self.type_name_registry.register::<R>();
        self.input_values.upsert::<R>(Box::new(R::new_resource(val)));
        if self.inputs.insert(TypeId::of::<R>()) {
            self.steps = None;
        }
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
            borrowed_inputs: N::list_borrowed_inputs(),
            outputs: N::list_outputs(),
        });

        self.steps = None;
    }

    fn construct_steps(&self) -> Result<GraphEvaluationSteps, Vec<GraphValidationError>> {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        enum InputOrNode {
            Input,
            Node(usize),
        }

        #[derive(Default)]
        struct UsedData {
            emitters: Vec<InputOrNode>,
            consumers: Vec<usize>,
            borrowers: Vec<usize>,
        }

        let mut resources_usage = HashMap::<TypeId, UsedData>::new();

        for &input in &self.inputs {
            let data = resources_usage.entry(input).or_default();
            data.emitters.push(InputOrNode::Input);
        }
        
        for (node_id, node) in self.nodes.iter().enumerate() {
            for &input in node.inputs {
                let data = resources_usage.entry(input).or_default();
                data.consumers.push(node_id);
            }
            for &borrowed_input in node.borrowed_inputs {
                let data = resources_usage.entry(borrowed_input).or_default();
                data.borrowers.push(node_id);
            }
            for &output in node.outputs {
                let data = resources_usage.entry(output).or_default();
                data.emitters.push(InputOrNode::Node(node_id));
            }
        }

        let mut errors = vec![];
        for (&tid, data) in &resources_usage {
            let resource_type_name = self.type_name_registry.get_name(tid).unwrap_or("unknown");
            let consumers = data.consumers.iter().map(|&idx| self.nodes[idx].type_id).map(|tid| self.type_name_registry.get_name(tid).unwrap_or("unknown")).collect_vec();
            let emitters = data.emitters.iter().map(|idx| match idx {
                InputOrNode::Input => "<input>",
                &InputOrNode::Node(idx) => self.type_name_registry.get_name(self.nodes[idx].type_id).unwrap_or("unknown"),
            }).collect_vec();

            if data.emitters.is_empty() {
                errors.push(GraphValidationError::ResourceLackEmitter {
                    resource_type_name,
                    consumers: consumers.clone(),
                });
            }
            if data.consumers.len() > 1 {
                errors.push(GraphValidationError::ResourceConsumedMultipleTimes {
                    resource_type_name,
                    consumers: consumers.clone(),
                });
            }
            if data.emitters.len() > 1 {
                errors.push(GraphValidationError::ResourceEmittedMultipleTimes {
                    resource_type_name,
                    emitters: emitters.clone(),
                });
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        // For every node, lists every node that must be computed before
        let mut back_links: Vec<HashSet<InputOrNode>> = vec![HashSet::default(); self.nodes.len()];

        for data in resources_usage.values() {
            let emitter = *data.emitters.first().unwrap();
            // Must use a value after its emitter
            for &n in std::iter::chain(&data.consumers, &data.borrowers) {
                back_links[n].insert(emitter);
            }
            // Must consume a value after all of its borrowers
            if let Some(&consumer) = data.consumers.first() {
                back_links[consumer].extend(data.borrowers.iter().copied().map(InputOrNode::Node));
            }
        }

        let mut remaining_nodes = (0..self.nodes.len()).collect_vec();
        // Nodes that have been pushed into output
        let mut pushed_nodes = HashSet::<InputOrNode>::new();
        pushed_nodes.insert(InputOrNode::Input);
        let mut output = Vec::<Box<[usize]>>::new();

        while !remaining_nodes.is_empty() {
            let nodes = remaining_nodes.extract_if(.., |&mut n| back_links[n].iter().all(|p| pushed_nodes.contains(p)));

            let mut new_pushed_nodes = pushed_nodes.clone();
            let mut new_output = Vec::<usize>::new();
            for node_idx in nodes {
                new_output.push(node_idx);
                new_pushed_nodes.insert(InputOrNode::Node(node_idx));
            }
            if new_pushed_nodes.is_empty() {
                return Err(vec![GraphValidationError::UnsatisfiableOrdering {  }]);
            }
            pushed_nodes = new_pushed_nodes;
            output.push(new_output.into_boxed_slice());
        }

        Ok(output.into_boxed_slice())
    }

    pub fn run(&mut self) -> ResourceStore {
        assert!(self.inputs.iter().copied().all(|p| self.input_values.has_any(p)), "Cannot run, missing inputs!");
        let mut resources = ResourceStore {
            resources: std::mem::take(&mut self.input_values),
        };
        let steps = match &self.steps {
            Some(steps) => steps,
            None => {
                let steps = match self.construct_steps() {
                    Ok(steps) => steps,
                    Err(e) => {
                        if let Some(path) = option_env!("DEBUG_GRAPH_PATH") {
                            std::fs::write(path, format!("{self}")).unwrap();
                            tracing::error!(output = path, errors = %e.iter().join(", "), "Errors while validating graph, written dot version in given path");
                        }

                        panic!("{e:#?}")
                    },
                };
                if let Some(path) = option_env!("DEBUG_GRAPH_PATH") {
                    self.steps = Some(steps);
                    std::fs::write(path, format!("{self}")).unwrap();
                    tracing::debug!(output = path, "Validated graph, written dot version in given path");
                    self.steps.as_ref().unwrap()
                }
                else {
                    &*self.steps.insert(steps)
                }
            },
        };
        for idx in steps.iter().flatten().copied() {
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

        if let Some(steps) = &self.steps {
            for [a, b] in steps.iter().flatten().copied().array_windows() {
                let name_a = get_name!(self.nodes[a].type_id, "N");
                let name_b = get_name!(self.nodes[b].type_id, "N");
                writeln!(f, "{INDENT}{name_a} -> {name_b} [color=blue]")?;
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
        declare_graph_deps!((R1,) -> (R2,));
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
    graph_resource!(struct Reversed(String));
    graph_resource!(struct ReversedHalf(String));
    graph_resource!(struct Half(String));
    graph_resource!(#[derive(Debug, PartialEq)] struct Output(String));

    struct Reserver;
    impl GraphNode for Reserver {
        declare_graph_deps!((ref Input,) -> (Reversed,));
        fn run((input,): (&String,)) -> (String,) {
            (input.chars().rev().collect(),)
        }
    }

    struct ReversedHalfer;
    impl GraphNode for ReversedHalfer {
        declare_graph_deps!((Reversed,) -> (ReversedHalf,));
        fn run((input,): (String,)) -> (String,) {
            let count = input.chars().count();
            let half = input.chars().take(count.div_ceil(2)).collect();
            (half,)
        }
    }

    struct InputHalfer;
    impl GraphNode for InputHalfer {
        declare_graph_deps!((ref Input,) -> (Half,));
        fn run((input,): (&String,)) -> (String,) {
            let count = input.chars().count();
            let half = input.chars().take(count.div_ceil(2)).collect();
            (half,)
        }
    }

    struct Concat;
    impl GraphNode for Concat {
        declare_graph_deps!((Half, ReversedHalf,) -> (Output,));
        fn run((a, b): (String, String)) -> (String,) {
            (a.chars().chain(b.chars()).collect(),)
        }
    }

    #[test]
    fn complex_construction() {
        let mut graph = RenderGraph::new();
        graph.push_node::<Concat>();
        graph.push_node::<ReversedHalfer>();
        graph.push_node::<Reserver>();
        graph.push_node::<InputHalfer>();
        graph.define_input::<Input>();
        assert_eq!(graph.construct_steps().unwrap().iter().map(|p| &**p).collect_vec(), vec![&[2,3][..],&[1],&[0]]);
    }

    #[test]
    fn complex_run() {
        let mut graph = RenderGraph::new();
        graph.push_node::<Reserver>();
        graph.push_node::<ReversedHalfer>();
        graph.push_node::<InputHalfer>();
        graph.push_node::<Concat>();
        graph.set_input::<Input>(format!("abcdefghi"));
        let output = graph.run();
        assert_eq!(output.resources.get::<Output>(), Some(&Output(format!("abcdeihgfe"))));
    }

    graph_resource!(struct Parrallel(u32));

    struct Emitter;
    impl GraphNode for Emitter {
        declare_graph_deps!(() -> (Parrallel,));

        fn run((): ()) -> (u32,) {
            (0,)
        }
    }

    struct Borrow1;
    impl GraphNode for Borrow1 {
        declare_graph_deps!((ref Parrallel,) -> ());
        fn run((_,): (&u32,)) -> () { }
    }
    struct Borrow2;
    impl GraphNode for Borrow2 {
        declare_graph_deps!((ref Parrallel,) -> ());
        fn run((_,): (&u32,)) -> () { }
    }

    struct Consumer;
    impl GraphNode for Consumer {
        declare_graph_deps!((Parrallel,) -> ());
        fn run((_,): (u32,)) -> () { }
    }

    #[test]
    fn parallel_borrow_contruct() {
        let mut graph = RenderGraph::new();
        graph.push_node::<Emitter>();
        graph.push_node::<Consumer>();
        graph.push_node::<Borrow1>();
        graph.push_node::<Borrow2>();
        assert_eq!(graph.construct_steps().unwrap().iter().map(|p| &**p).collect_vec(), vec![&[0][..],&[2,3],&[1]]);
    }
}
