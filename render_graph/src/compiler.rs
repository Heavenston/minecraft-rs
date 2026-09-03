use std::{any::TypeId, collections::{HashMap, HashSet}, convert::identity, rc::Rc, sync::{Arc}};

use itertools::{Itertools as _, chain};
use parking_lot::RwLock;

use crate::RenderGraph;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum InputOrNode {
    Input,
    Node(usize),
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ResolvedInput {
    resource: TypeId,
    producer: InputOrNode,
}

type Producers = HashMap<TypeId, Vec<InputOrNode>>;

fn compute_producers(graph: &RenderGraph) -> Producers {
    std::iter::chain(
        graph.nodes.iter().enumerate().flat_map(|(node_id, p)| p.outputs.iter().map(move |&tid| (tid, InputOrNode::Node(node_id)))),
        graph.inputs.iter().map(|&tid| (tid, InputOrNode::Input)),
    ).into_group_map()
}

#[derive(Default, Clone)]
struct ListLink(Option<Rc<Cons>>);
struct Cons {
    val: InputOrNode,
    prev: ListLink,
}

impl ListLink {
    fn iter(&self) -> impl Iterator<Item = InputOrNode> {
        struct Iter<'a>(Option<&'a Cons>);
        impl<'a> Iterator for Iter<'a> {
            type Item = InputOrNode;

            fn next(&mut self) -> Option<Self::Item> {
                match self.0 {
                    Some(n) => {
                        let val = n.val;
                        self.0 = n.prev.0.as_ref().map(|p| &**p);
                        Some(val)
                    },
                    None => None,
                }
            }
        }
        Iter(self.0.as_ref().map(|p| &**p))
    }

    fn cons(self, val: InputOrNode) -> Self {
        Self(Some(Rc::new(Cons {
            val,
            prev: self,
        })))
    }
}

trait ResolvedResourcesContainer {
    fn node_count(&self) -> usize;
    fn explicit_orderings(&self) -> &[(usize, usize)];
    fn resolved_inputs(&self, id: usize) -> impl Iterator<Item = &ResolvedInput>;
    fn resolved_borrows(&self, id: usize) -> impl Iterator<Item = &ResolvedInput>;

    fn combined_inputs(&self, i: usize) -> impl Iterator<Item = &ResolvedInput> {
        chain!(self.resolved_inputs(i), self.resolved_borrows(i))
    }

    fn borrowers_of(&self, i: &ResolvedInput) -> impl Iterator<Item = usize> {
        (0..self.node_count())
            .filter(move |&node_id| self.resolved_borrows(node_id).any(move |ri| ri == i))
    }

    #[tracing::instrument(skip(self, recursive))]
    fn is_after_or_equal(&self, recursive: ListLink, after: InputOrNode, before: usize) -> bool {
        if recursive.iter().any(|o| o == after) {
            tracing::warn!("recursive");
            return false;
        }
        let recursive = recursive.cons(after);

        let after = match after { InputOrNode::Input => return false, InputOrNode::Node(node) => node };

        after == before ||
        self.explicit_orderings().contains(&(before, after)) ||
        self.combined_inputs(after).any(|input| input.producer == InputOrNode::Node(before)) ||
        self.resolved_inputs(after).any(|input1| self.resolved_borrows(before).any(|input2| input1 == input2)) ||
        self.combined_inputs(after)
            .any(|input| {
                tracing::debug_span!("input", ?input.resource).in_scope(|| {
                    self.is_after_or_equal(recursive.clone(), input.producer, before)
                })
            }) ||
        self.resolved_inputs(after)
            .flat_map(|i| self.borrowers_of(i))
            .any(|o| {
                tracing::debug_span!("cross-borrow").in_scope(|| {
                    self.is_after_or_equal(recursive.clone(), InputOrNode::Node(o), before)
                })
            })
    }
}

struct ResolvedResources {
    explicit_orderings: Box<[(usize, usize)]>,
    inputs: Box<[Box<[ResolvedInput]>]>,
    borrows: Box<[Box<[ResolvedInput]>]>,
    consumers: HashMap<ResolvedInput, usize>,
    borrowers: HashMap<ResolvedInput, Vec<usize>>
}

impl ResolvedResources {
    fn users(&self, res: &ResolvedInput) -> impl Iterator<Item = usize> {
        chain!(
            self.consumers.get(res),
            self.borrowers.get(res).map(Vec::as_slice).unwrap_or_default(),
        ).copied()
    }

    fn write_to_dot(&self, graph: &RenderGraph, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        const INDENT: &'static str = "  ";

        writeln!(f, "digraph {{")?;
        let mut names = HashMap::<&'static str, HashMap<TypeId, String>>::new();
        macro_rules! tn { ($t:expr) => {
            graph.type_name_registry.get_name($t)
        }; }
        let mut printed_resources = HashSet::<TypeId>::new();
        macro_rules! get_name { ($n: expr, $pref: expr) => {{
            let p = names.entry($pref).or_default();
            let c = p.len();
            p.entry($n).or_insert_with(|| format!("{}{c}", $pref)).clone()
        }}; }
        macro_rules! write_resource { ($tid:expr) => {{
            let tid = $tid;
            if printed_resources.insert(tid) {
                let type_name = graph.type_name_registry.get_name(tid);
                write_node!(get_name!(tid, "R"), shape=>"rectangle", label=>type_name);
            }
        }}; }
        macro_rules! write_node { ($name:expr$(,$key:expr=>$val:expr)*) => {{
            write!(f, "{INDENT}{} [", $name)?;
            $(write!(f, " {}=\"{}\"", stringify!($key), $val)?;)*
            writeln!(f, "]")?;
        }}; }
        macro_rules! write_edge {
            ($i:ident -> $n:ident$(,$key:expr=>$val:expr)*) => {{
                let name = $n.clone();
                let input = $i;
                match input.producer {
                    InputOrNode::Input => {
                        write_resource!($i.resource);
                        write!(f, "{INDENT}{} -> {name} [", get_name!(input.resource, "R"))?;
                    },
                    InputOrNode::Node(n) => {
                        write!(f, "{INDENT}{} -> {name} [label=\"{}\"", format!("N{n}"), tn!(input.resource))?;
                    }
                };
                $(write!(f, " {}=\"{}\"", stringify!($key), $val)?;)*
                writeln!(f, "]")?;
            }};
        }

        for (i, n) in graph.nodes.iter().enumerate().sorted_by_key(|(_, n)| n.label()) {
            let name = format!("N{i}");
            write_node!(name, shape=>"cylinder", label=>format!("{i} {}", n.label()));
            for input in self.resolved_inputs(i) {
                write_edge!(input -> name);
            }
            for input in self.resolved_borrows(i) {
                write_edge!(input -> name, style=>"dashed");
            }
        }
        
        write!(f, "}}")?;

        Ok(())
    }
}

impl ResolvedResourcesContainer for ResolvedResources {
    fn node_count(&self) -> usize {
        self.inputs.len()
    }
    fn explicit_orderings(&self) -> &[(usize, usize)] {
        &self.explicit_orderings
    }
    fn resolved_inputs(&self, id: usize) -> impl Iterator<Item = &ResolvedInput> {
        self.inputs[id].iter()
    }
    fn resolved_borrows(&self, id: usize) -> impl Iterator<Item = &ResolvedInput> {
        self.borrows[id].iter()
    }
}

struct GraphResourceResolver {
    explicit_orderings: Box<[(usize, usize)]>,
    resolved_inputs: Box<[Box<[Option<ResolvedInput>]>]>,
    resolved_borrows: Box<[Box<[Option<ResolvedInput>]>]>,
}

impl GraphResourceResolver {
    fn is_complete(&self) -> bool {
        self.resolved_inputs.iter().flatten().all(|p| p.is_some()) && self.resolved_borrows.iter().flatten().all(|p| p.is_some())
    }

    fn consumer(&self, ri: &ResolvedInput) -> Option<usize> {
        (0..self.node_count())
            .filter(move |&node_i| self.resolved_inputs(node_i).any(move |p| p == ri))
            .at_most_one().expect("There should always only be at most one consumer")
    }

    fn consumers(&self) -> HashMap<ResolvedInput, usize> {
        (0..self.node_count())
            .flat_map(|i| self.resolved_inputs(i).map(move |input| (input.clone(), i)))
            .into_grouping_map()
            .reduce(|a, _key, b| panic!("cannot be two consumer {a} and {b}"))
    }

    fn borrowers(&self) -> HashMap<ResolvedInput, Vec<usize>> {
        (0..self.node_count())
            .flat_map(|i| self.resolved_borrows(i).map(move |input| (input.clone(), i)))
            .into_group_map()
    }
}

impl ResolvedResourcesContainer for GraphResourceResolver {
    fn node_count(&self) -> usize {
        self.resolved_inputs.len()
    }
    fn explicit_orderings(&self) -> &[(usize, usize)] {
        &self.explicit_orderings
    }
    fn resolved_inputs(&self, id: usize) -> impl Iterator<Item = &ResolvedInput> {
        self.resolved_inputs[id].iter().filter_map(Option::as_ref)
    }
    fn resolved_borrows(&self, id: usize) -> impl Iterator<Item = &ResolvedInput> {
        self.resolved_borrows[id].iter().filter_map(Option::as_ref)
    }
}

#[tracing::instrument(level = "trace", skip_all)]
fn resolve_resources(graph: &RenderGraph) -> ResolvedResources {
    macro_rules! tn {
        ($t:expr) => {
            graph.type_name_registry.get_name($t)
        };
    }
    macro_rules! nn {
        ($n:expr) => {
            tn!(graph.nodes.values()[$n].type_id())
        };
    }
    macro_rules! ton {
        ($n:expr) => {
            match $n {
                InputOrNode::Input => "<input>",
                InputOrNode::Node(n) => nn!(n),
            }
        };
    }

    macro_rules! rr_println {
        ($($t:tt)*) => {
            if let Some(_) = option_env!("ENABLE_GRAPH_COMPILER_DEBUG") {
                println!($($t)*);
            }
        };
    }

    let mut producers = compute_producers(graph);
    let producers_for_borrows = compute_producers(graph);
    let explicit_orderings: Box<[(usize, usize)]> = graph.explicit_orderings.iter()
        .map(|&(before, after)| (graph.nodes.get_index(before.0).unwrap(), graph.nodes.get_index(after.0).unwrap()))
        .collect_vec().into_boxed_slice();
    let nodes = graph.nodes.values();
    let mut this = GraphResourceResolver {
        explicit_orderings,
        resolved_inputs: graph.nodes.iter().map(|node| vec![None; node.inputs.len()]).map_into().collect(),
        resolved_borrows: graph.nodes.iter().map(|node| vec![None; node.borrowed_inputs.len()]).map_into().collect(),
    };

    while !this.is_complete() {
        rr_println!("\n\n##############################");
        #[derive(Default)]
        struct ClaimList {
            borrows: Vec<(usize, usize)>,
            inputs: Vec<(usize, usize)>,
        }
        let mut claims = HashMap::<(TypeId, InputOrNode), ClaimList>::new();
        for (node_id, node) in nodes.iter().enumerate() {
            rr_println!("{}[{node_id}] {}", chain!(&this.resolved_inputs[node_id], &this.resolved_borrows[node_id]).any(Option::is_none).then_some(" ").unwrap_or("*"), nn!(node_id));
            for (input_idx, input) in node.inputs.iter().enumerate() {
                rr_println!("\tconsumes({})", tn!(*input));

                if let Some(resolved) = &this.resolved_inputs[node_id][input_idx] {
                    rr_println!("\t\t*{}", ton!(resolved.producer));
                    continue;
                };
                let Some(producers) = producers.get(&input)
                else { panic!("Missing producers for {}", tn!(*input)) };

                let producer = producers.iter().filter(|&&producer| {
                    !this.is_after_or_equal(Default::default(), producer, node_id)
                }).exactly_one();

                match producer {
                    Ok(&p) => {
                        rr_println!("\t\t{}", ton!(p));
                        claims.entry((*input, p)).or_default().inputs.push((node_id, input_idx))
                    },
                    Err(options) => {
                        rr_println!("\t\t{}", options.map(|n| ton!(*n)).join(", "));
                    },
                }
            }
            for (borrow_idx, borrow_input) in node.borrowed_inputs.iter().enumerate() {
                rr_println!("\tborrows({})", tn!(*borrow_input));

                if let Some(resolved) = &this.resolved_borrows[node_id][borrow_idx] {
                    rr_println!("\t\t*{}", ton!(resolved.producer));
                    continue
                }
                let Some(producers) = producers_for_borrows.get(&borrow_input)
                else { panic!("Missing producers for {}", tn!(*borrow_input)) };

                let producer = producers.iter().filter(|&&producer| {
                    !this.is_after_or_equal(Default::default(), producer, node_id)
                }).filter(|&&producer| {
                    this.consumer(&ResolvedInput { resource: *borrow_input, producer: producer })
                        .is_none_or(|p| !this.is_after_or_equal(Default::default(), InputOrNode::Node(node_id), p))
                }).exactly_one();

                match producer {
                    Ok(&p) => {
                        rr_println!("\t\t{}", ton!(p));
                        claims.entry((*borrow_input, p)).or_default().borrows.push((node_id, borrow_idx))
                    },
                    Err(options) => {
                        rr_println!("\t\t{}", options.map(|n| ton!(*n)).join(", "));
                    },
                }
            }
        }

        let mut found_valid = false;
        for ((claim_tid, claim_producer), claimers) in claims {
            if claimers.borrows.is_empty() {
                if let Ok(&(node_id, input_idx)) = claimers.inputs.iter().exactly_one() {
                    rr_println!("REVOLED {} consumes {} from {}", nn!(node_id), tn!(claim_tid), ton!(claim_producer));
                    found_valid = true;
                    debug_assert!(this.resolved_inputs[node_id][input_idx].is_none());
                    this.resolved_inputs[node_id][input_idx] = Some(ResolvedInput {
                        resource: claim_tid,
                        producer: claim_producer,
                    });
                    producers.get_mut(&claim_tid).unwrap().retain(|&p| p != claim_producer);
                }
                else {
                    tracing::warn!(?claim_tid, ?claim_producer, ?claimers.inputs, "Consumer conflict");
                    for &(a, _) in &claimers.inputs {
                        for &(b, _) in &claimers.inputs {
                            if a == b { continue }
                            println!("{a} is before {b} : {}", this.is_after_or_equal(Default::default(), InputOrNode::Node(b), a))
                        }
                    }
                }
            }
            else {
                found_valid = true;
                for (node_id, input_idx) in claimers.borrows {
                    rr_println!("REVOLED {} borrows {} from {}", nn!(node_id), tn!(claim_tid), ton!(claim_producer));
                    debug_assert!(this.resolved_borrows[node_id][input_idx].is_none());
                    this.resolved_borrows[node_id][input_idx] = Some(ResolvedInput {
                        resource: claim_tid,
                        producer: claim_producer,
                    });
                }
            }
        }
        if !found_valid {
            panic!("Could not contruct render graph ordering");
        }
    }

    ResolvedResources {
        consumers: this.consumers(),
        borrowers: this.borrowers(),
        explicit_orderings: this.explicit_orderings,
        inputs: this.resolved_inputs.into_iter().map(|p| p.into_iter().map(Option::unwrap).collect()).collect(),
        borrows: this.resolved_borrows.into_iter().map(|p| p.into_iter().map(Option::unwrap).collect()).collect(),
    }
}

fn compute_total_order(graph: &RenderGraph, resolved: &ResolvedResources) -> Box<[usize]> {
    tracing::debug!("Computing graph total order");
    let mut successors: Vec<HashSet<usize>> = vec![HashSet::new(); graph.nodes.len()];
    for (node_idx, input) in chain!(resolved.borrows.iter().enumerate(), resolved.inputs.iter().enumerate()).flat_map(|(node_idx, inputs)| inputs.iter().map(move |input| (node_idx, input))) {
        if let InputOrNode::Node(node_idx2) = input.producer {
            successors[node_idx2].insert(node_idx);
        }
    }
    for (node_idx, borrows) in resolved.borrows.iter().enumerate() {
        for borrow in borrows.iter() {
            let consumer = resolved.inputs.iter().enumerate().flat_map(|(node_idx, inputs)| inputs.iter().map(move |input| (node_idx, input)))
                .find(|&(_,p)| p == borrow);
            if let Some((consumer,_)) = consumer {
                successors[node_idx].insert(consumer);
            }
        }
    }
    let successors = successors;

    let mut done = vec![false; graph.nodes.len()];
    let mut output = Vec::<usize>::new();
    while !done.iter().copied().all(identity) {
        let Some((node, _)) = successors.iter().enumerate().filter(|&(i, _)| !done[i]).find(|(_,succ)| succ.iter().find(|&&o| !done[o]).is_none())
        else { panic!("Could not resolve graph ordering") };
        done[node] = true;
        output.push(node);
    }
    output.reverse();

    output.into()
}

#[derive(Default)]
pub struct ComputeResult {
    pub required_inputs: Box<[TypeId]>,
    pub steps: Box<[usize]>,
    output_new_permanents: Box<[TypeId]>,
}

pub struct CompiledGraph {
    producers: Producers,
    resolved: ResolvedResources,
    total_order: Box<[usize]>,
    /// Only contains `permanent` resources
    not_dirty: Vec<TypeId>,
    cache: RwLock<HashMap<Vec<TypeId>, Arc<ComputeResult>>>,
}

impl CompiledGraph {
    pub fn new(graph: &RenderGraph) -> Self {
        let producers = compute_producers(graph);
        for (k, v) in &producers {
            if graph.is_resource_permanent(k) {
                assert!(v.len() == 1, "Permanent resources must have exactly one producer")
            }
        }
        let resolved = resolve_resources(graph);
        if let Some(path) = option_env!("DEBUG_GRAPH_PATH") {
            let mut str = String::new();
            resolved.write_to_dot(graph, &mut str).unwrap();
            std::fs::write(path, str).unwrap();
            tracing::debug!(output = path, "RUN graph, written dot version in given path");
        }

        let total_order = compute_total_order(graph, &resolved);
        Self {
            producers,
            resolved,
            total_order,
            not_dirty: Default::default(),
            cache: RwLock::new(HashMap::new()),
        }
    }

    fn mark_dirty_rec(not_dirty: &mut Vec<TypeId>, resolved: &ResolvedResources, graph: &RenderGraph, res: ResolvedInput) {
        if graph.is_resource_permanent(&res.resource) {
            if let Some(idx) = not_dirty.iter().position(|p| p == &res.resource) {
                not_dirty.swap_remove(idx);
            }
        }
        for user in resolved.users(&res) {
            for output in graph.nodes.values()[user].outputs {
                Self::mark_dirty_rec(not_dirty, resolved, graph, ResolvedInput {
                    resource: *output,
                    producer: InputOrNode::Node(user),
                });
            }
        }
    }

    pub fn mark_resource_dirty(&mut self, graph: &RenderGraph, resource: TypeId) {
        assert!(graph.is_resource_permanent(&resource), "Only permanent resources can bbe marked dirty");
        let &[producer] = self.producers.get(&resource).map(Vec::as_slice).unwrap_or_default()
        else { panic!("Permanent resources only have one producer") };
        Self::mark_dirty_rec(&mut self.not_dirty, &self.resolved, graph, ResolvedInput { resource, producer });
        self.not_dirty.sort_unstable();
    }

    fn is_dirty(&self, graph: &RenderGraph, res: &ResolvedInput) -> bool {
        if graph.is_resource_permanent(&res.resource) {
            !self.not_dirty.contains(&res.resource)
        }
        else {
            true
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn compute(&self, graph: &RenderGraph, tid: TypeId) -> Arc<ComputeResult> {
        if let Some(cached) = self.cache.read().get(&self.not_dirty).map(Arc::clone) {
            return cached;
        }
        tracing::trace!("Cache miss");

        assert!(graph.is_resource_permanent(&tid), "Can only compute a permanent resource");
        let &[InputOrNode::Node(producer)] = self.producers.get(&tid).map(Vec::as_slice).unwrap_or_default()
        else { panic!("Can only compute if there is exactly one producer of a resource, and that producer isn't the input"); };
        if !self.is_dirty(graph, &ResolvedInput { resource: tid, producer: InputOrNode::Node(producer) }) {
            return Arc::new(ComputeResult::default());
        }

        let mut required_inputs = HashSet::<TypeId>::new();
        let mut done = vec![false; graph.nodes.len()];
        let mut needed = vec![false; graph.nodes.len()];
        let mut stack = vec![];
        let mut created: Vec<TypeId> = vec![];

        stack.push(producer);
        done[producer] = true;
        needed[producer] = true;

        while let Some(node_idx) = stack.pop() {
            for input in self.resolved.combined_inputs(node_idx) {
                match input.producer {
                    InputOrNode::Input => {
                        required_inputs.insert(input.resource);
                    },
                    InputOrNode::Node(n) => if !done[n] {
                        done[n] = true;
                        if graph.is_resource_permanent(&input.resource) {
                            if !self.not_dirty.contains(&input.resource) {
                                created.push(input.resource);
                                needed[n] = true;
                                stack.push(n);
                            }
                        }
                        else {
                            needed[n] = true;
                            stack.push(n);
                        }
                    },
                }
            }
        }

        assert!(created.iter().all_unique());

        let result = Arc::new(ComputeResult {
            required_inputs: required_inputs.into_iter().collect(),
            steps: self.total_order.iter().copied().filter(|&p| needed[p]).collect(),
            output_new_permanents: created.into(),
        });
        debug_assert!(self.not_dirty.is_sorted());
        self.cache.write().insert(self.not_dirty.clone(), Arc::clone(&result));

        if let Some(path) = option_env!("DEBUG_GRAPH_COMPUTE_PATH") {
            let mut str = String::new();
            graph.write_to_dot(&mut str, Some(&result.steps)).unwrap();
            std::fs::write(path, str).unwrap();
            tracing::debug!(output = path, "Stored compute() dot graph");
        }

        result
    }

    pub fn apply_compute_result(&mut self, _graph: &RenderGraph, result: &ComputeResult) {
        self.not_dirty.extend(&result.output_new_permanents);
        debug_assert!(self.not_dirty.iter().all_unique());
        self.not_dirty.sort_unstable();
    }
}
