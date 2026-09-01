#![feature(macro_metavar_expr)]

use std::{any::{ Any, TypeId }, collections::{HashMap, HashSet}, convert::identity};
use itertools::{Itertools as _, chain};
use typemap::TypeMap;

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
enum GraphValidationError { }

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

    fn construct_steps(&self) -> Result<GraphEvaluationSteps, GraphValidationError> {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        enum InputOrNode {
            Input,
            Node(usize),
        }
        #[derive(Debug, Clone, PartialEq, Eq)]
        struct ResolvedInput {
            resource: TypeId,
            producer: InputOrNode,
        }

        let mut producers = std::iter::chain(
            self.nodes.iter().enumerate().flat_map(|(node_id, p)| p.outputs.iter().map(move |&tid| (tid, InputOrNode::Node(node_id)))),
            self.inputs.iter().map(|&tid| (tid, InputOrNode::Input)),
        ).into_group_map();
        type ResolvedInputs = Box<[Box<[Option<ResolvedInput>]>]>;
        let mut resolved_inputs: ResolvedInputs = self.nodes.iter().map(|node| vec![None; node.inputs.len()]).map_into().collect_vec().into();
        let mut resolved_borrows: ResolvedInputs = self.nodes.iter().map(|node| vec![None; node.borrowed_inputs.len()]).map_into().collect_vec().into();

        fn is_after_or_equal(graph: &RenderGraph, resolved_inputs: &ResolvedInputs, resolved_borrows: &ResolvedInputs, after: InputOrNode, before: usize) -> bool {
            let after = match after { InputOrNode::Input => return false, InputOrNode::Node(node) => node };
            if after == before { return true; }

            let consumes_borrow = resolved_borrows[before].iter()
                .filter_map(Option::as_ref)
                .any(|a| resolved_inputs[after].iter().filter_map(Option::as_ref).any(|b| a == b));

            if consumes_borrow {
                return true;
            }

            chain!(
                resolved_inputs[after].iter(),
                resolved_borrows[after].iter(),
            ).filter_map(Option::as_ref)
                .any(|input| is_after_or_equal(graph, resolved_inputs, resolved_borrows, input.producer, before))
        }

        while !resolved_inputs.iter().flatten().all(|p| p.is_some()) || !resolved_borrows.iter().flatten().all(|p| p.is_some()) {
            #[derive(Default)]
            struct ClaimList {
                borrows: Vec<(usize, usize)>,
                inputs: Vec<(usize, usize)>,
            }
            let mut claims = HashMap::<(TypeId, InputOrNode), ClaimList>::new();
            for (node_id, node) in self.nodes.iter().enumerate() {
                for (input_idx, input) in node.inputs.iter().enumerate() {
                    if resolved_inputs[node_id][input_idx].is_some() { continue };
                    let Some(producers) = producers.get_mut(&input)
                    else { panic!("Missing producers") };

                    let producer = producers.iter().filter(|&&producer| {
                        !is_after_or_equal(self, &resolved_inputs, &resolved_borrows, producer, node_id)
                    }).exactly_one();

                    match producer {
                        Ok(&p) => claims.entry((*input, p)).or_default().inputs.push((node_id, input_idx)),
                        Err(_) => (),
                    }
                }
                for (borrow_idx, borrow_input) in node.borrowed_inputs.iter().enumerate() {
                    if resolved_borrows[node_id][borrow_idx].is_some() { continue };
                    let Some(producers) = producers.get_mut(&borrow_input)
                    else { panic!("Missing producers") };

                    let producer = producers.iter().filter(|&&producer| {
                        !is_after_or_equal(self, &resolved_inputs, &resolved_borrows, producer, node_id)
                    }).exactly_one();

                    match producer {
                        Ok(&p) => claims.entry((*borrow_input, p)).or_default().borrows.push((node_id, borrow_idx)),
                        Err(_) => (),
                    }
                }
            }

            let mut found_valid = false;
            for ((claim_tid, claim_producer), claimers) in claims {
                if claimers.borrows.is_empty() {
                    if let Ok(&(node_id, input_idx)) = claimers.inputs.iter().exactly_one() {
                        found_valid = true;
                        debug_assert!(resolved_inputs[node_id][input_idx].is_none());
                        resolved_inputs[node_id][input_idx] = Some(ResolvedInput {
                            resource: claim_tid,
                            producer: claim_producer,
                        });
                        producers.get_mut(&claim_tid).unwrap().retain(|&p| p != claim_producer);
                    }
                }
                else {
                    found_valid = true;
                    for (node_id, input_idx) in claimers.borrows {
                        debug_assert!(resolved_borrows[node_id][input_idx].is_none());
                        resolved_borrows[node_id][input_idx] = Some(ResolvedInput {
                            resource: claim_tid,
                            producer: claim_producer,
                        });
                    }
                }
            }
            if !found_valid {
                panic!("dd");
            }
        }

        let mut successors: Vec<HashSet<usize>> = vec![HashSet::new(); self.nodes.len()];
        for (node_idx, input) in chain!(resolved_borrows.iter().enumerate(), resolved_inputs.iter().enumerate()).flat_map(|(node_idx, inputs)| inputs.iter().filter_map(Option::as_ref).map(move |input| (node_idx, input))) {
            if let InputOrNode::Node(node_idx2) = input.producer {
                successors[node_idx2].insert(node_idx);
            }
        }
        for (node_idx, borrows) in resolved_borrows.iter().enumerate() {
            for borrow in borrows.iter().filter_map(Option::as_ref) {
                let consumer = resolved_inputs.iter().enumerate().flat_map(|(node_idx, inputs)| inputs.iter().filter_map(Option::as_ref).map(move |input| (node_idx, input)))
                    .find(|&(_,p)| p == borrow);
                if let Some((consumer,_)) = consumer {
                    successors[node_idx].insert(consumer);
                }
            }
        }
        let successors = successors;

        let mut done = vec![false; self.nodes.len()];
        let mut output = Vec::<usize>::new();
        while !done.iter().copied().all(identity) {
            let Some((node, _)) = successors.iter().enumerate().filter(|&(i, _)| !done[i]).find(|(_,succ)| succ.iter().find(|&&o| !done[o]).is_none())
            else { panic!("Could not resolve graph ordering") };
            done[node] = true;
            output.push(node);
        }
        output.reverse();
        Ok(output.into())        
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
            if let Some(type_name) = self.type_name_registry.try_get_name(id) {
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

        for &t in self.inputs.iter().sorted_by_key(|p| self.type_name_registry.get_name(**p)) {
            write_node!(t, "R", "rectangle");
        }
        for (i, n) in self.nodes.iter().enumerate().sorted_by_key(|(_, n)| self.type_name_registry.get_name(n.type_id)) {
            write_node!(n.type_id, "N", "cylinder", format!("{i} "));
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
        assert_matches!(&graph.construct_steps().unwrap()[..], &[2,1,3,0] | &[3,2,1,0]);
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
        assert_matches!(&graph.construct_steps().unwrap()[..], &[0,2,3,1] | &[0,3,2,1]);
    }

    #[test]
    fn parallel_borrow_run() {
        let mut graph = RenderGraph::new();
        graph.push_node::<Emitter>();
        graph.push_node::<Consumer>();
        graph.push_node::<Borrow1>();
        graph.push_node::<Borrow2>();
        graph.run();
    }
}
