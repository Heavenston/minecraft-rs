#![feature(macro_metavar_expr)]

use std::{any::{ Any, TypeId }, collections::{HashMap, HashSet}, ops::ControlFlow};
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
            $(registry.register::<$borrowed_input>();)*
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
    #[error("Could not found satisfying graph ordering")]
    UnsatisfiableOrdering { },
}

type GraphEvaluationSteps = Box<[usize]>;

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

        let inputs = N::list_inputs();
        let borrowed_inputs = N::list_borrowed_inputs();
        let shared = inputs.iter().filter(|o| borrowed_inputs.contains(o)).collect_vec();
        assert!(shared.is_empty(), "Error pushing Node {} into render graph, the following resources are both consumed and borrowed: {shared:?}", std::any::type_name::<N>());

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

    fn recursive_stepper(
        &self,
        output: Vec<usize>,
        available_resources: HashSet<TypeId>,
        remaining_nodes: Vec<usize>,
    ) -> ControlFlow<GraphEvaluationSteps> {
        if remaining_nodes.is_empty() {
            return ControlFlow::Break(output.into_boxed_slice());
        }

        for &n in &remaining_nodes {
            let inputs_available = std::iter::chain(self.nodes[n].inputs, self.nodes[n].borrowed_inputs)
                .all(|p| available_resources.contains(p));
            let outputs_non_present = self.nodes[n].outputs.iter()
                // A resource taken then outputed by the same node is exempt here
                .filter(|p| !self.nodes[n].inputs.contains(p))
                .all(|p| !available_resources.contains(p));
            let runnable = inputs_available && outputs_non_present;

            if runnable {
                let mut output = output.clone();
                output.push(n);
                let mut available_resources = available_resources.clone();
                for i in self.nodes[n].inputs {
                    available_resources.remove(i);
                }
                available_resources.extend(self.nodes[n].outputs);
                let remaining_nodes = remaining_nodes.iter().copied().filter(|&p| p != n).collect();
                self.recursive_stepper(output, available_resources, remaining_nodes)?;
            }
        }
        ControlFlow::Continue(())
    }

    fn construct_steps(&self) -> Result<GraphEvaluationSteps, GraphValidationError> {
        let result = self.recursive_stepper(vec![], self.inputs.clone(), (0..self.nodes.len()).collect_vec());
        match result {
            ControlFlow::Continue(()) => Err(GraphValidationError::UnsatisfiableOrdering {  }),
            ControlFlow::Break(steps) => Ok(steps),
        }
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
                    Err(error) => {
                        if let Some(path) = option_env!("DEBUG_GRAPH_PATH") {
                            std::fs::write(path, format!("{self}")).unwrap();
                            tracing::error!(output = path, %error, "Errors while validating graph, written dot version in given path");
                        }
                        panic!("{error:#?}")
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
        for idx in steps.iter().copied() {
            (self.nodes[idx].run)(&mut resources);
        }
        self.input_values = Default::default();
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
        macro_rules! write_node { ($id:expr,$pref:expr,$shape:expr$(,$name_prepend:expr)?) => {{
            let id = $id;
            printed_resources.insert(id);
            let name = get_name!(id, $pref);
            write!(f, "{INDENT}{name} [shape={}", $shape)?;
            if let Some(type_name) = self.type_name_registry.get_name(id) {
                write!(f, " label=\"")?;
                $(write!(f, "{}", $name_prepend)?;)?
                write!(f, "{type_name}\"")?;
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
        for (i, n) in self.nodes.iter().enumerate() {
            write_node!(n.type_id, "N", "diamond", format!("{i} "));
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
            for [a, b] in steps.iter().copied().array_windows() {
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
        assert_eq!(&graph.construct_steps().unwrap()[..], &[2,1,3,0]);
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
        graph.run();
        assert_eq!(&graph.construct_steps().unwrap()[..], &[0,2,3,1]);
    }
}
