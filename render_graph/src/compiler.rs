use std::{collections::{HashMap, HashSet}, convert::identity, rc::Rc, sync::{Arc}};

use genmap::DenseIdx;
use indexmap::{IndexMap, IndexSlice, MapIndex as _};
use itertools::{Itertools as _, chain};
use parking_lot::RwLock;

use crate::{NodeData, RenderGraph, ResourceData, UntypedNodeHandle, UntypedResourceHandle};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct NodeRef(DenseIdx);

impl indexmap::MapIndex for NodeRef {
    fn from_usize(idx: usize) -> Self {
        Self(DenseIdx::from_usize(idx))
    }

    fn as_usize(&self) -> usize {
        self.0.as_usize()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ResourceRef(DenseIdx);

impl indexmap::MapIndex for ResourceRef {
    fn from_usize(idx: usize) -> Self {
        Self(DenseIdx::from_usize(idx))
    }

    fn as_usize(&self) -> usize {
        self.0.as_usize()
    }
}

struct NodeDataWrapper<'a> {
    data: &'a NodeData,
    graph: &'a RenderGraph,
}

impl<'a> NodeDataWrapper<'a> {
    fn label(&self) -> &'a str {
        self.data.node.label()
    }

    fn borrows(&self) -> impl Iterator<Item = ResourceRef> + use<'a> {
        self.data.borrows.iter().map(|handle| self.graph.resource_ref(*handle))
    }

    fn consumes(&self) -> impl Iterator<Item = ResourceRef> + use<'a> {
        self.data.consumes.iter().map(|handle| self.graph.resource_ref(*handle))
    }

    fn outputs(&self) -> impl Iterator<Item = ResourceRef> + use<'a> {
        self.data.outputs.iter().map(|handle| self.graph.resource_ref(*handle))
    }
}

trait RenderGraphExt {
    fn nodes(&self) -> impl Iterator<Item = (NodeRef, NodeDataWrapper<'_>)>;
    fn node(&self, node: &NodeRef) -> NodeDataWrapper<'_>;
    fn resources(&self) -> &IndexSlice<ResourceData, ResourceRef>;
    fn node_ref(&self, handle: impl Into<UntypedNodeHandle>) -> NodeRef;
    fn node_handle(&self, node: &NodeRef) -> UntypedNodeHandle;
    fn resource_ref(&self, resource: impl Into<UntypedResourceHandle>) -> ResourceRef;
    fn resource_handle(&self, resource: &ResourceRef) -> UntypedResourceHandle;
    fn is_resource_ref_permanent(&self, resource: &ResourceRef) -> bool;
}

impl RenderGraphExt for RenderGraph {
    fn nodes(&self) -> impl Iterator<Item = (NodeRef, NodeDataWrapper<'_>)> {
        self.nodes.values().as_with_index::<NodeRef>().enumerated()
            .map(|(node, data)| (node, NodeDataWrapper { data, graph: self }))
    }

    fn node(&self, node: &NodeRef) -> NodeDataWrapper<'_> {
        let data = &self.nodes.values()[&node.0];
        NodeDataWrapper {
            data,
            graph: self,
        }
    }

    fn resources(&self) -> &IndexSlice<ResourceData, ResourceRef> {
        self.resources.values().as_with_index()
    }

    fn node_ref(&self, handle: impl Into<UntypedNodeHandle>) -> NodeRef {
        NodeRef(self.nodes.get_dense_index(handle.into().0).expect("Invalid resource handle"))
    }

    fn node_handle(&self, node: &NodeRef) -> UntypedNodeHandle {
        UntypedNodeHandle(self.nodes.from_dense_index(node.0).expect("valid dense index"))
    }

    fn resource_ref(&self, resource: impl Into<UntypedResourceHandle>) -> ResourceRef {
        ResourceRef(self.resources.get_dense_index(resource.into().0).expect("Invalid resource handle"))
    }

    fn resource_handle(&self, resource: &ResourceRef) -> UntypedResourceHandle {
        UntypedResourceHandle(self.resources.from_dense_index(resource.0).expect("valid dense index"))
    }

    fn is_resource_ref_permanent(&self, resource: &ResourceRef) -> bool {
        self.resources()[resource].is_permanent
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum InputOrNode {
    Input,
    Node(NodeRef),
}

impl PartialEq<NodeRef> for InputOrNode {
    fn eq(&self, other: &NodeRef) -> bool {
        match self {
            Self::Input => false,
            Self::Node(node_ref) => node_ref == other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ResolvedInput {
    resource: ResourceRef,
    producer: InputOrNode,
}

type Producers = HashMap<ResourceRef, Vec<InputOrNode>>;

fn compute_producers(graph: &RenderGraph) -> Producers {
    std::iter::chain(
        graph.nodes()
            .flat_map(|(node_id, node_data)| node_data.outputs()
                .map(move |resource| (resource, InputOrNode::Node(node_id.clone())))
            ),
        graph.inputs.iter().map(|&tid| (graph.resource_ref(tid), InputOrNode::Input)),
    ).into_group_map()
}

#[derive(Default, Clone)]
struct ListLink(Option<Rc<Cons>>);
struct Cons {
    val: InputOrNode,
    prev: ListLink,
}

impl ListLink {
    fn iter(&self) -> impl Iterator<Item = &InputOrNode> {
        struct Iter<'a>(Option<&'a Cons>);
        impl<'a> Iterator for Iter<'a> {
            type Item = &'a InputOrNode;

            fn next(&mut self) -> Option<Self::Item> {
                match self.0 {
                    Some(n) => {
                        let val = &n.val;
                        self.0 = n.prev.0.as_deref();
                        Some(val)
                    },
                    None => None,
                }
            }
        }
        Iter(self.0.as_deref())
    }

    fn cons(self, val: InputOrNode) -> Self {
        Self(Some(Rc::new(Cons {
            val,
            prev: self,
        })))
    }
}

trait ResolvedResourcesContainer {
    fn node_refs(&self) -> impl Iterator<Item = NodeRef> + ExactSizeIterator + DoubleEndedIterator;
    fn explicit_orderings(&self) -> &[(NodeRef, NodeRef)];
    fn resolved_inputs(&self, node: &NodeRef) -> impl Iterator<Item = &ResolvedInput> + use<'_, Self>;
    fn resolved_borrows(&self, node: &NodeRef) -> impl Iterator<Item = &ResolvedInput> + use<'_, Self>;

    fn combined_inputs(&self, i: &NodeRef) -> impl Iterator<Item = &ResolvedInput> {
        chain!(self.resolved_inputs(i), self.resolved_borrows(i))
    }

    fn borrowers_of(&self, ri: &ResolvedInput) -> impl Iterator<Item = NodeRef> {
        self.node_refs().filter(move |node2| self.resolved_borrows(node2).any(move |ri2| ri2 == ri))
    }

    #[tracing::instrument(skip(self, recursive))]
    fn is_after_or_equal(&self, recursive: ListLink, after: &InputOrNode, before: &NodeRef) -> bool {
        if recursive.iter().any(|o| o == after) {
            tracing::warn!("recursive");
            return false;
        }
        let recursive = recursive.cons(after.clone());

        let after = match after { InputOrNode::Input => return false, InputOrNode::Node(node) => node };

        after == before ||
        self.explicit_orderings().contains(&(before.clone(), after.clone())) ||
        self.combined_inputs(after).any(|input| &input.producer == before) ||
        self.resolved_inputs(after).any(|input1| self.resolved_borrows(before).any(|input2| input1 == input2)) ||
        self.combined_inputs(after)
            .any(|input| {
                tracing::debug_span!("input", ?input.resource).in_scope(|| {
                    self.is_after_or_equal(recursive.clone(), &input.producer, before)
                })
            }) ||
        self.resolved_inputs(after)
            .flat_map(|i| self.borrowers_of(i))
            .any(|o| {
                tracing::debug_span!("cross-borrow").in_scope(|| {
                    self.is_after_or_equal(recursive.clone(), &InputOrNode::Node(o), before)
                })
            })
    }
}

struct ResolvedResources {
    node_count: usize,
    explicit_orderings: Box<[(NodeRef, NodeRef)]>,
    inputs: IndexMap<Box<[ResolvedInput]>, NodeRef>,
    borrows: IndexMap<Box<[ResolvedInput]>, NodeRef>,
    consumers: HashMap<ResolvedInput, NodeRef>,
    borrowers: HashMap<ResolvedInput, Vec<NodeRef>>
}

impl ResolvedResources {
    fn users(&self, res: &ResolvedInput) -> impl Iterator<Item = &NodeRef> {
        chain!(
            self.consumers.get(res),
            self.borrowers.get(res).map(Vec::as_slice).unwrap_or_default(),
        )
    }

    fn write_to_dot(&self, graph: &RenderGraph, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        let _ = graph;
        let _ = f;
        todo!()
        // const INDENT: &'static str = "  ";

        // writeln!(f, "digraph {{")?;
        // let mut names = HashMap::<&'static str, HashMap<TypeId, String>>::new();
        // macro_rules! tn { ($t:expr) => {
        //     graph.type_name_registry.get_name($t)
        // }; }
        // let mut printed_resources = HashSet::<TypeId>::new();
        // macro_rules! get_name { ($n: expr, $pref: expr) => {{
        //     let p = names.entry($pref).or_default();
        //     let c = p.len();
        //     p.entry($n).or_insert_with(|| format!("{}{c}", $pref)).clone()
        // }}; }
        // macro_rules! write_resource { ($tid:expr) => {{
        //     let tid = $tid;
        //     if printed_resources.insert(tid) {
        //         let type_name = graph.type_name_registry.get_name(tid);
        //         write_node!(get_name!(tid, "R"), shape=>"rectangle", label=>type_name);
        //     }
        // }}; }
        // macro_rules! write_node { ($name:expr$(,$key:expr=>$val:expr)*) => {{
        //     write!(f, "{INDENT}{} [", $name)?;
        //     $(write!(f, " {}=\"{}\"", stringify!($key), $val)?;)*
        //     writeln!(f, "]")?;
        // }}; }
        // macro_rules! write_edge {
        //     ($i:ident -> $n:ident$(,$key:expr=>$val:expr)*) => {{
        //         let name = $n.clone();
        //         let input = $i;
        //         match input.producer {
        //             InputOrNode::Input => {
        //                 write_resource!($i.resource);
        //                 write!(f, "{INDENT}{} -> {name} [", get_name!(input.resource, "R"))?;
        //             },
        //             InputOrNode::Node(n) => {
        //                 write!(f, "{INDENT}{} -> {name} [label=\"{}\"", format!("N{n}"), tn!(input.resource))?;
        //             }
        //         };
        //         $(write!(f, " {}=\"{}\"", stringify!($key), $val)?;)*
        //         writeln!(f, "]")?;
        //     }};
        // }

        // for (i, n) in graph.nodes.iter().enumerate().sorted_by_key(|(_, n)| n.label()) {
        //     let name = format!("N{i}");
        //     write_node!(name, shape=>"cylinder", label=>format!("{i} {}", n.label()));
        //     for input in self.resolved_inputs(i) {
        //         write_edge!(input -> name);
        //     }
        //     for input in self.resolved_borrows(i) {
        //         write_edge!(input -> name, style=>"dashed");
        //     }
        // }
        
        // write!(f, "}}")?;

        // Ok(())
    }
}

impl ResolvedResourcesContainer for ResolvedResources {
    fn node_refs(&self) -> impl Iterator<Item = NodeRef> + ExactSizeIterator + DoubleEndedIterator {
        (0..self.node_count).map(|i| NodeRef(DenseIdx::from_usize(i)))
    }
    fn explicit_orderings(&self) -> &[(NodeRef, NodeRef)] {
        &self.explicit_orderings
    }
    fn resolved_inputs(&self, node: &NodeRef) -> impl Iterator<Item = &ResolvedInput> + use<'_> {
        self.inputs[node].iter()
    }
    fn resolved_borrows(&self, node: &NodeRef) -> impl Iterator<Item = &ResolvedInput> + use<'_> {
        self.borrows[node].iter()
    }
}

struct GraphResourceResolver {
    node_refs: Box<[NodeRef]>,
    explicit_orderings: Box<[(NodeRef, NodeRef)]>,
    resolved_inputs: IndexMap<Box<[Option<ResolvedInput>]>, NodeRef>,
    resolved_borrows: IndexMap<Box<[Option<ResolvedInput>]>, NodeRef>,
}

impl GraphResourceResolver {
    fn is_complete(&self) -> bool {
        self.resolved_inputs.iter().flatten().all(std::option::Option::is_some) && self.resolved_borrows.iter().flatten().all(std::option::Option::is_some)
    }

    fn consumer(&self, ri: &ResolvedInput) -> Option<NodeRef> {
        self.node_refs()
            .filter(move |node_i| self.resolved_inputs(node_i).any(move |p| p == ri))
            .at_most_one().ok().expect("There should always only be at most one consumer")
    }

    fn consumers(&self) -> HashMap<ResolvedInput, NodeRef> {
        self.node_refs()
            .flat_map(|i| self.resolved_inputs(&i.clone()).map(move |input| (input.clone(), i.clone())))
            .into_grouping_map()
            .reduce(|a, _key, b| panic!("cannot be two consumer {a:?} and {b:?}"))
    }

    fn borrowers(&self) -> HashMap<ResolvedInput, Vec<NodeRef>> {
        self.node_refs()
            .flat_map(|i| self.resolved_borrows(&i).map(move |input| (input.clone(), i.clone())))
            .into_group_map()
    }
}

impl ResolvedResourcesContainer for GraphResourceResolver {
    fn node_refs(&self) -> impl Iterator<Item = NodeRef> + ExactSizeIterator + DoubleEndedIterator {
        self.node_refs.iter().cloned()
    }
    fn explicit_orderings(&self) -> &[(NodeRef, NodeRef)] {
        &self.explicit_orderings
    }
    fn resolved_inputs(&self, node: &NodeRef) -> impl Iterator<Item = &ResolvedInput> + use<'_> {
        self.resolved_inputs[node].iter().filter_map(Option::as_ref)
    }
    fn resolved_borrows(&self, node: &NodeRef) -> impl Iterator<Item = &ResolvedInput> + use<'_> {
        self.resolved_borrows[node].iter().filter_map(Option::as_ref)
    }
}

#[tracing::instrument(level = "trace", skip_all)]
#[expect(clippy::single_call_fn, reason = "CompiledGraph construction algorithm")]
fn resolve_resources(graph: &RenderGraph) -> ResolvedResources {
    macro_rules! tn {
        ($t:expr) => {{
            // graph.type_name_registry.get_name($t)
            let _ = $t;
            ""
        }};
    }
    macro_rules! nn {
        ($n:expr) => {
            tn!(graph.node($n).label())
        };
    }
    macro_rules! ton {
        ($n:expr) => {
            match &$n {
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
    let producers_for_borrows = producers.clone();
    let explicit_orderings: Box<[(NodeRef, NodeRef)]> = graph.explicit_orderings.iter()
        .map(|&(before, after)| (graph.node_ref(before), graph.node_ref(after)))
        .collect_vec().into_boxed_slice();
    let mut this = GraphResourceResolver {
        node_refs: graph.nodes.iter_dense_indexes().map(NodeRef).collect(),
        explicit_orderings,
        resolved_inputs: graph.nodes.iter().map(|node| vec![None; node.consumes.len()]).map_into().collect(),
        resolved_borrows: graph.nodes.iter().map(|node| vec![None; node.borrows.len()]).map_into().collect(),
    };

    while !this.is_complete() {
        rr_println!("\n\n##############################");
        #[derive(Default)]
        struct ClaimList {
            borrows: Vec<(NodeRef, usize)>,
            inputs: Vec<(NodeRef, usize)>,
        }
        let mut claims = HashMap::<(ResourceRef, InputOrNode), ClaimList>::new();
        for (node, node_data) in graph.nodes() {
            rr_println!("{}[{node:?}] {}", if chain!(&this.resolved_inputs[&node], &this.resolved_borrows[&node]).any(Option::is_none) { " " } else { "*" }, nn!(&node));
            for (input_idx, resource) in node_data.consumes().enumerate() {
                rr_println!("\tconsumes({})", tn!(resource));

                if let Some(resolved) = &this.resolved_inputs[&node][input_idx] {
                    rr_println!("\t\t*{}", ton!(resolved.producer));
                    continue;
                }
                let Some(potential_producers) = producers.get(&resource)
                else { panic!("Missing producers for {}", tn!(resource)) };

                let producer = potential_producers.iter().filter(|&producer| {
                    !this.is_after_or_equal(ListLink::default(), producer, &node)
                }).exactly_one();

                match producer {
                    Ok(p) => {
                        rr_println!("\t\t{}", ton!(p));
                        claims.entry((resource, p.clone())).or_default().inputs.push((node.clone(), input_idx));
                    },
                    Err(options) => {
                        rr_println!("\t\t{}", options.map(|n| ton!(*n)).join(", "));
                    },
                }
            }
            for (borrow_idx, resource) in node_data.borrows().enumerate() {
                rr_println!("\tborrows({})", tn!(resource));

                if let Some(resolved) = &this.resolved_borrows[&node][borrow_idx] {
                    rr_println!("\t\t*{}", ton!(resolved.producer));
                    continue
                }
                let Some(potential_producers) = producers_for_borrows.get(&resource)
                else { panic!("Missing producers for {}", tn!(resource)) };

                let producer = potential_producers.iter().filter(|&producer| {
                    !this.is_after_or_equal(ListLink::default(), producer, &node)
                }).filter(|&producer| {
                    this.consumer(&ResolvedInput { resource: resource.clone(), producer: producer.clone() })
                        .is_none_or(|p| !this.is_after_or_equal(ListLink::default(), &InputOrNode::Node(node.clone()), &p))
                }).exactly_one();

                match producer {
                    Ok(p) => {
                        rr_println!("\t\t{}", ton!(p));
                        claims.entry((resource, p.clone())).or_default().borrows.push((node.clone(), borrow_idx));
                    },
                    Err(options) => {
                        rr_println!("\t\t{}", options.map(|n| ton!(*n)).join(", "));
                    },
                }
            }
        }

        let mut found_valid = false;
        #[expect(clippy::iter_over_hash_type, reason = "Iteration order here has no impact")]
        for ((claim_resource, claim_producer), claimers) in claims {
            if claimers.borrows.is_empty() {
                if let Ok(&(ref node_id, input_idx)) = claimers.inputs.iter().exactly_one() {
                    rr_println!("REVOLED {} consumes {} from {}", nn!(node_id), tn!(claim_resource), ton!(claim_producer));
                    found_valid = true;
                    debug_assert!(this.resolved_inputs[node_id][input_idx].is_none());
                    this.resolved_inputs[node_id][input_idx] = Some(ResolvedInput {
                        resource: claim_resource.clone(),
                        producer: claim_producer.clone(),
                    });
                    producers.get_mut(&claim_resource).unwrap().retain(|p| p != &claim_producer);
                }
                else {
                    tracing::warn!(?claim_resource, ?claim_producer, ?claimers.inputs, "Consumer conflict");
                    for (a, _) in &claimers.inputs {
                        for (b, _) in &claimers.inputs {
                            if a == b { continue }
                            println!("{a:?} is before {b:?} : {}", this.is_after_or_equal(ListLink::default(), &InputOrNode::Node(b.clone()), a));
                        }
                    }
                }
            }
            else {
                found_valid = true;
                for (node_id, input_idx) in claimers.borrows {
                    rr_println!("REVOLED {} borrows {} from {}", nn!(&node_id), tn!(claim_resource), ton!(claim_producer));
                    debug_assert!(this.resolved_borrows[&node_id][input_idx].is_none());
                    this.resolved_borrows[&node_id][input_idx] = Some(ResolvedInput {
                        resource: claim_resource.clone(),
                        producer: claim_producer.clone(),
                    });
                }
            }
        }
        assert!(found_valid, "Could not contruct render graph ordering");
    }

    ResolvedResources {
        node_count: graph.nodes.len(),
        consumers: this.consumers(),
        borrowers: this.borrowers(),
        explicit_orderings: this.explicit_orderings,
        inputs: this.resolved_inputs.into_iter().map(|p| p.into_iter().map(Option::unwrap).collect()).collect(),
        borrows: this.resolved_borrows.into_iter().map(|p| p.into_iter().map(Option::unwrap).collect()).collect(),
    }
}

#[expect(clippy::single_call_fn, reason = "CompiledGraph construction algorithm")]
fn compute_total_order(graph: &RenderGraph, resolved: &ResolvedResources) -> Box<[NodeRef]> {
    tracing::debug!("Computing graph total order");
    let mut successors: IndexMap<HashSet<NodeRef>, NodeRef> = vec![HashSet::new(); graph.nodes.len()].into();
    for (node_idx, input) in chain!(resolved.borrows.enumerated(), resolved.inputs.enumerated()).flat_map(|(node_idx, inputs)| inputs.iter().map(move |input| (node_idx.clone(), input))) {
        if let InputOrNode::Node(node_idx2) = &input.producer {
            successors[node_idx2].insert(node_idx);
        }
    }
    for (node, borrows) in resolved.borrows.enumerated() {
        for borrow in borrows {
            let consumer = resolved.inputs.enumerated().flat_map(|(consumer, inputs)| inputs.iter().map(move |input| (consumer.clone(), input)))
                .find(|&(_,p)| p == borrow);
            if let Some((consumer,_)) = consumer {
                successors[&node].insert(consumer);
            }
        }
    }
    let successors = successors;

    let mut done = IndexMap::<bool, NodeRef>::from(vec![false; graph.nodes.len()]);
    let mut output = Vec::<NodeRef>::new();
    while !done.iter().copied().all(identity) {
        let Some((node, _)) = successors.enumerated().filter(|(i, _)| !done[i]).find(|(_,succ)| !succ.iter().any(|o| !done[o]))
        else { panic!("Could not resolve graph ordering") };
        done[&node] = true;
        output.push(node);
    }
    output.reverse();

    output.into()
}

#[derive(Default)]
pub struct ComputeResult {
    pub required_inputs: Box<[UntypedResourceHandle]>,
    pub steps: Box<[UntypedNodeHandle]>,
    output_new_permanents: Box<[ResourceRef]>,
}

pub struct CompiledGraph {
    producers: Producers,
    resolved: ResolvedResources,
    total_order: Box<[NodeRef]>,
    /// Only contains `permanent` resources.
    not_dirty: Vec<ResourceRef>,
    cache: RwLock<HashMap<Vec<ResourceRef>, Arc<ComputeResult>>>,
}

impl CompiledGraph {
    #[expect(clippy::single_call_fn, reason = "Meant to encapsulate a single usage")]
    pub fn new(graph: &RenderGraph) -> Self {
        let producers = compute_producers(graph);
        assert!(
            producers.iter()
                .filter(|(k, _)| graph.is_resource_ref_permanent(k))
                .all(|(_, v)| v.len() == 1),
            "Permanent resources must have exactly one producer"
        );
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
            not_dirty: Vec::default(),
            cache: RwLock::new(HashMap::new()),
        }
    }

    fn mark_dirty_rec(not_dirty: &mut Vec<ResourceRef>, resolved: &ResolvedResources, graph: &RenderGraph, res: &ResolvedInput) {
        if graph.is_resource_ref_permanent(&res.resource)
            && let Some(idx) = not_dirty.iter().position(|p| p == &res.resource) {
                not_dirty.swap_remove(idx);
            }
        for user in resolved.users(res) {
            for output in graph.node(user).outputs() {
                Self::mark_dirty_rec(not_dirty, resolved, graph, &ResolvedInput {
                    resource: output,
                    producer: InputOrNode::Node(user.clone()),
                });
            }
        }
    }

    pub fn mark_resource_dirty(&mut self, graph: &RenderGraph, resource: UntypedResourceHandle) {
        let resource = graph.resource_ref(resource);
        assert!(graph.is_resource_ref_permanent(&resource), "Only permanent resources can bbe marked dirty");
        let [producer] = self.producers.get(&resource).map(Vec::as_slice).unwrap_or_default()
        else { panic!("Permanent resources only have one producer") };
        Self::mark_dirty_rec(&mut self.not_dirty, &self.resolved, graph, &ResolvedInput { resource, producer: producer.clone() });
        self.not_dirty.sort_unstable_by_key(indexmap::MapIndex::as_usize);
    }

    fn is_dirty(&self, graph: &RenderGraph, res: &ResolvedInput) -> bool {
        if graph.is_resource_ref_permanent(&res.resource) {
            !self.not_dirty.contains(&res.resource)
        }
        else {
            true
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn compute(&self, graph: &RenderGraph, resource: UntypedResourceHandle) -> Arc<ComputeResult> {
        let resource = graph.resource_ref(resource);
        let cached = self.cache.read().get(&self.not_dirty).map(Arc::clone);
        if let Some(cached) = cached {
            return cached;
        }
        tracing::trace!("Cache miss");

        assert!(graph.is_resource_ref_permanent(&resource), "Can only compute a permanent resource");
        let [InputOrNode::Node(producer)] = self.producers.get(&resource).map(Vec::as_slice).unwrap_or_default()
        else { panic!("Can only compute if there is exactly one producer of a resource, and that producer isn't the input"); };
        if !self.is_dirty(graph, &ResolvedInput { resource: resource.clone(), producer: InputOrNode::Node(producer.clone()) }) {
            return Arc::new(ComputeResult::default());
        }

        let mut required_inputs = HashSet::<ResourceRef>::new();
        let mut done = IndexMap::<bool, NodeRef>::from(vec![false; graph.nodes.len()]);
        let mut needed = IndexMap::<bool, NodeRef>::from(vec![false; graph.nodes.len()]);
        let mut stack = Vec::<&NodeRef>::new();
        let mut created: Vec<ResourceRef> = vec![];

        stack.push(producer);
        done[producer] = true;
        needed[producer] = true;

        while let Some(node_idx) = stack.pop() {
            for input in self.resolved.combined_inputs(node_idx) {
                match &input.producer {
                    InputOrNode::Input => {
                        required_inputs.insert(input.resource.clone());
                    },
                    InputOrNode::Node(n) => if !done[n] {
                        done[n] = true;
                        if graph.is_resource_ref_permanent(&input.resource) {
                            if !self.not_dirty.contains(&input.resource) {
                                created.push(input.resource.clone());
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
            required_inputs: required_inputs.into_iter().map(|r| graph.resource_handle(&r)).collect(),
            steps: self.total_order.iter().filter(|&p| needed[p]).map(|node| graph.node_handle(node)).collect(),
            output_new_permanents: created.into(),
        });
        debug_assert!(self.not_dirty.is_sorted_by_key(indexmap::MapIndex::as_usize));
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
        self.not_dirty.extend(result.output_new_permanents.iter().cloned());
        debug_assert!(self.not_dirty.iter().all_unique());
        self.not_dirty.sort_unstable_by_key(indexmap::MapIndex::as_usize);
    }
}
