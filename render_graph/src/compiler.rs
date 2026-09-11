use std::{cell::RefCell, collections::{HashMap, HashSet}, convert::identity, hash::Hash, ops::ControlFlow, rc::Rc, sync::Arc};

use genmap::{AssumeAlive, DenseIdx};
use indexmap::{IndexMap, IndexSlice, MapIndex as _};
use itertools::{Either, Itertools as _, chain};
use ordermap::OrderMap;
use parking_lot::RwLock;
#[cfg(debug_assertions)]
use rand::{SeedableRng as _, seq::{IndexedRandom as _, IteratorRandom as _}};

use crate::{Label, NodeData, RenderGraph, ResourceConfig, ResourceData, UncheckedNodeHandle, UncheckedResourceHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NodeRef(DenseIdx);

impl indexmap::MapIndex for NodeRef {
    fn from_usize(idx: usize) -> Self {
        Self(DenseIdx::from_usize(idx))
    }

    fn as_usize(&self) -> usize {
        self.0.as_usize()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ResourceRef(DenseIdx);

impl indexmap::MapIndex for ResourceRef {
    fn from_usize(idx: usize) -> Self {
        Self(DenseIdx::from_usize(idx))
    }

    fn as_usize(&self) -> usize {
        self.0.as_usize()
    }
}

/// Wrapper to provide access to a node input/outputs as `ResourceRef`.
struct NodeDataWrapper<'a> {
    data: &'a NodeData,
    graph: &'a RenderGraph,
}

impl<'a> NodeDataWrapper<'a> {
    fn label(&self) -> Label<'a> {
        self.data.node.label()
    }

    fn borrows(&self) -> impl Iterator<Item = ResourceRef> + use<'a> {
        self.data.borrows.iter().map(|&handle| self.graph.resource_ref(handle))
    }

    fn consumes(&self) -> impl Iterator<Item = ResourceRef> + use<'a> {
        self.data.consumes.iter().map(|&handle| self.graph.resource_ref(handle))
    }

    fn outputs(&self) -> impl Iterator<Item = ResourceRef> + use<'a> {
        self.data.outputs.iter().map(|&handle| self.graph.resource_ref(handle))
    }

    fn is_mutator(&self, resource: ResourceRef) -> bool {
        self.consumes().contains(&resource) && self.outputs().contains(&resource)
    }
}

trait RenderGraphExt {
    fn nodes(&self) -> impl Iterator<Item = (NodeRef, NodeDataWrapper<'_>)>;
    fn node(&self, node: NodeRef) -> NodeDataWrapper<'_>;
    fn resources(&self) -> &IndexSlice<ResourceData, ResourceRef>;
    fn resource_cfg(&self, resource: ResourceRef) -> &ResourceConfig;
    fn node_ref(&self, handle: impl Into<UncheckedNodeHandle>) -> NodeRef;
    fn node_handle(&self, node: NodeRef) -> UncheckedNodeHandle;
    fn resource_ref(&self, resource: impl Into<UncheckedResourceHandle>) -> ResourceRef;
    fn resource_handle(&self, resource: ResourceRef) -> UncheckedResourceHandle;
    fn is_resource_ref_permanent(&self, resource: ResourceRef) -> bool;
}

impl RenderGraphExt for RenderGraph {
    fn nodes(&self) -> impl Iterator<Item = (NodeRef, NodeDataWrapper<'_>)> {
        self.nodes.values().as_with_index::<NodeRef>().enumerated()
            .map(|(node, data)| (node, NodeDataWrapper { data, graph: self }))
    }

    fn node(&self, node: NodeRef) -> NodeDataWrapper<'_> {
        let data = &self.nodes.values()[&node.0];
        NodeDataWrapper {
            data,
            graph: self,
        }
    }

    fn resources(&self) -> &IndexSlice<ResourceData, ResourceRef> {
        self.resources.values().as_with_index()
    }

    fn resource_cfg(&self, resource: ResourceRef) -> &ResourceConfig {
        &self.resources.with(resource.0).expect("valid dense index").get().config
    }

    fn node_ref(&self, handle: impl Into<UncheckedNodeHandle>) -> NodeRef {
        NodeRef(self.nodes.with(AssumeAlive(handle.into().0)).dense_idx())
    }

    fn node_handle(&self, node: NodeRef) -> UncheckedNodeHandle {
        UncheckedNodeHandle(self.nodes.with(node.0).expect("valid dense index").sparse_idx())
    }

    fn resource_ref(&self, resource: impl Into<UncheckedResourceHandle>) -> ResourceRef {
        ResourceRef(self.resources.with(AssumeAlive(resource.into().0)).dense_idx())
    }

    fn resource_handle(&self, resource: ResourceRef) -> UncheckedResourceHandle {
        UncheckedResourceHandle(self.resources.with(resource.0).unwrap().sparse_idx())
    }

    fn is_resource_ref_permanent(&self, resource: ResourceRef) -> bool {
        self.resources()[resource].config.permanent
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ResolvedInput {
    resource: ResourceRef,
    producer: InputOrNode,
}

type Producers = HashMap<ResourceRef, Vec<InputOrNode>>;

fn compute_producers(graph: &RenderGraph) -> Producers {
    std::iter::chain(
        graph.nodes()
            .flat_map(|(node_id, node_data)| node_data.outputs()
                .map(move |resource| (resource, InputOrNode::Node(node_id)))
            ),
        graph.inputs.iter().map(|&tid| (graph.resource_ref(tid), InputOrNode::Input)),
    ).into_group_map()
}

trait ResolvedResourcesContainer {
    fn node_refs(&self) -> impl Iterator<Item = NodeRef> + ExactSizeIterator + DoubleEndedIterator;
    fn explicit_orderings(&self) -> &[(NodeRef, NodeRef)];
    fn resolved_inputs(&self, node: NodeRef) -> impl Iterator<Item = ResolvedInput> + use<'_, Self>;
    fn resolved_borrows(&self, node: NodeRef) -> impl Iterator<Item = ResolvedInput> + use<'_, Self>;

    fn combined_inputs(&self, i: NodeRef) -> impl Iterator<Item = ResolvedInput> {
        chain!(self.resolved_inputs(i), self.resolved_borrows(i))
    }

    fn borrowers_of(&self, ri: ResolvedInput) -> impl Iterator<Item = NodeRef> {
        self.node_refs().filter(move |&node2| self.resolved_borrows(node2).any(move |ri2| ri2 == ri))
    }
}

#[derive(Debug)]
struct ResolvedResources {
    node_count: usize,
    explicit_orderings: Box<[(NodeRef, NodeRef)]>,
    inputs: IndexMap<Box<[ResolvedInput]>, NodeRef>,
    borrows: IndexMap<Box<[ResolvedInput]>, NodeRef>,
    consumers: HashMap<ResolvedInput, NodeRef>,
    borrowers: HashMap<ResolvedInput, Vec<NodeRef>>,
}

impl ResolvedResources {
    fn find_loop_rec(&self, current: NodeRef, mut previous: Vec<InputOrNode>) -> ControlFlow<Vec<InputOrNode>> {
        previous.push(InputOrNode::Node(current));
        for i in self.combined_inputs(current) {
            if previous.contains(&i.producer) {
                previous.push(i.producer);
                return ControlFlow::Break(previous);
            }
            if let InputOrNode::Node(n) = i.producer {
                self.find_loop_rec(n, previous.clone())?;
            }
        }

        ControlFlow::Continue(())
    }

    fn find_loop(&self) -> Option<Vec<InputOrNode>> {
        for nr in self.node_refs() {
            if let Ok(found) = self.find_loop_rec(nr, vec![]).break_ok() {
                return Some(found);
            }
        }
        None
    }

    fn users(&self, res: ResolvedInput) -> impl Iterator<Item = NodeRef> {
        chain!(
            self.consumers.get(&res),
            self.borrowers.get(&res).map(Vec::as_slice).unwrap_or_default(),
        ).copied()
    }

    fn write_to_dot(&self, graph: &RenderGraph, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        let node_labels = graph.get_simplified_node_labels();
        let resource_labels = graph.get_simplified_resource_labels();

        const INDENT: &str = "  ";

        macro_rules! rn {
            ($t:expr) => {{
                // &graph.resources.with($t.0).unwrap().get().label
                resource_labels[&graph.resource_handle($t)].clone()
            }};
        }

        writeln!(f, "digraph {{")?;
        let mut names = HashMap::<&'static str, HashMap<ResourceRef, String>>::new();
        let mut printed_resources = HashSet::<ResourceRef>::new();
        macro_rules! get_name { ($n: expr, $pref: expr) => {{
            let p = names.entry($pref).or_default();
            let c = p.len();
            p.entry($n).or_insert_with(|| format!("{}{c}", $pref)).clone()
        }}; }
        macro_rules! write_resource { ($resource:expr) => {{
            let resource = $resource;
            if printed_resources.insert(resource) {
                write_node!(get_name!(resource, "R"), shape=>"rectangle", label=>rn!(resource));
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
                        write!(f, "{INDENT}{} -> {name} [label=\"{}\"", format!("N{}", n.as_usize()), rn!(input.resource))?;
                    }
                };
                $(write!(f, " {}=\"{}\"", stringify!($key), $val)?;)*
                writeln!(f, "]")?;
            }};
        }

        for (i,_) in graph.nodes().sorted_by_key(|(_,n)| n.label()) {
            let name = format!("N{}", i.as_usize());
            write_node!(name, shape=>"cylinder", label=>node_labels[&graph.node_handle(i)]);
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
    fn node_refs(&self) -> impl Iterator<Item = NodeRef> + ExactSizeIterator + DoubleEndedIterator {
        (0..self.node_count).map(|i| NodeRef(DenseIdx::from_usize(i)))
    }
    fn explicit_orderings(&self) -> &[(NodeRef, NodeRef)] {
        &self.explicit_orderings
    }
    fn resolved_inputs(&self, node: NodeRef) -> impl Iterator<Item = ResolvedInput> + use<'_> {
        self.inputs[node].iter().copied()
    }
    fn resolved_borrows(&self, node: NodeRef) -> impl Iterator<Item = ResolvedInput> + use<'_> {
        self.borrows[node].iter().copied()
    }
}

#[derive(Debug)]
struct GraphResourceResolver {
    node_refs: Box<[NodeRef]>,
    explicit_orderings: Box<[(NodeRef, NodeRef)]>,
    resolved_inputs: IndexMap<Box<[Option<ResolvedInput>]>, NodeRef>,
    resolved_borrows: IndexMap<Box<[Option<ResolvedInput>]>, NodeRef>,

    after_or_equal_cache: RefCell<HashSet<(InputOrNode, NodeRef)>>,
    after_or_equal_cache_inv: RefCell<HashSet<(InputOrNode, NodeRef)>>,
}

impl GraphResourceResolver {
    fn is_complete(&self) -> bool {
        self.resolved_inputs.iter().flatten().all(std::option::Option::is_some) && self.resolved_borrows.iter().flatten().all(std::option::Option::is_some)
    }

    fn consumer(&self, ri: ResolvedInput) -> Option<NodeRef> {
        self.node_refs()
            .filter(move |&node_i| self.resolved_inputs(node_i).any(move |p| p == ri))
            .at_most_one().ok().expect("There should always only be at most one consumer")
    }

    fn consumers(&self) -> HashMap<ResolvedInput, NodeRef> {
        self.node_refs()
            .flat_map(|i| self.resolved_inputs(i).map(move |input| (input, i)))
            .into_grouping_map()
            .reduce(|a, _key, b| panic!("cannot be two consumer {a:?} and {b:?}"))
    }

    fn borrowers(&self) -> HashMap<ResolvedInput, Vec<NodeRef>> {
        self.node_refs()
            .flat_map(|i| self.resolved_borrows(i).map(move |input| (input, i)))
            .into_group_map()
    }

    fn is_after_or_equal_rec(&self, mut recursive: HashSet<(InputOrNode, NodeRef)>, after: InputOrNode, before: NodeRef) -> bool {
        if self.after_or_equal_cache.borrow().contains(&(after, before)) { return true; }
        if self.after_or_equal_cache_inv.borrow().contains(&(after, before)) { return false; }
        if recursive.contains(&(after, before)) {
            return false;
        }

        recursive.insert((after, before));
        let after = match after { InputOrNode::Input => return false, InputOrNode::Node(node) => node };

        let result = after == before || (
            !self.resolved_inputs[after].iter().all(Option::is_none) && !self.resolved_inputs[before].iter().all(Option::is_none) && (
            self.explicit_orderings().contains(&(before, after)) ||
            self.combined_inputs(after).any(|input| input.producer == before) ||
            self.resolved_inputs(after).any(|input1| self.resolved_borrows(before).any(|input2| input1 == input2)) ||
            self.node_refs().any(|third| self.is_after_or_equal_rec(recursive.clone(), InputOrNode::Node(after), third) && self.is_after_or_equal_rec(recursive.clone(), InputOrNode::Node(third), before))
        ));
        if result {
            let mut cache = self.after_or_equal_cache.borrow_mut();
            cache.insert((InputOrNode::Node(before), after));
            for after2 in cache.iter().filter(|&&(_,before2)| before2 == after).map(|&(after2,_)| after2).collect_vec() {
                cache.insert((after2, before));
            }
        }
        else {
            self.after_or_equal_cache_inv.borrow_mut().insert((InputOrNode::Node(before), after));
        }
        result
    }

    fn is_after_or_equal(&self, after: InputOrNode, before: NodeRef) -> bool {
        self.is_after_or_equal_rec(Default::default(), after, before)
    }
}

impl ResolvedResourcesContainer for GraphResourceResolver {
    fn node_refs(&self) -> impl Iterator<Item = NodeRef> + ExactSizeIterator + DoubleEndedIterator {
        self.node_refs.iter().copied()
    }
    fn explicit_orderings(&self) -> &[(NodeRef, NodeRef)] {
        &self.explicit_orderings
    }
    fn resolved_inputs(&self, node: NodeRef) -> impl Iterator<Item = ResolvedInput> + use<'_> {
        self.resolved_inputs[node].iter().filter_map(Option::as_ref).copied()
    }
    fn resolved_borrows(&self, node: NodeRef) -> impl Iterator<Item = ResolvedInput> + use<'_> {
        self.resolved_borrows[node].iter().filter_map(Option::as_ref).copied()
    }
}

#[tracing::instrument(level = "trace", skip_all)]
#[expect(clippy::single_call_fn, reason = "CompiledGraph construction algorithm")]
fn resolve_resources(graph: &RenderGraph) -> Option<ResolvedResources> {
    let node_labels = graph.get_simplified_node_labels();
    let resource_labels = graph.get_simplified_resource_labels();
    macro_rules! rn {
        ($t:expr) => {{
            // &graph.resources.with($t.0).unwrap().get().label
            resource_labels[&graph.resource_handle($t)].clone()
        }};
    }
    macro_rules! nn {
        ($n:expr) => {
            node_labels.get(&graph.node_handle($n)).cloned().unwrap_or_default()
        };
    }
    macro_rules! ton {
        ($n:expr) => {
            match $n {
                InputOrNode::Input => "<input>".to_string(),
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
    macro_rules! rr_print {
        ($($t:tt)*) => {
            if let Some(_) = option_env!("ENABLE_GRAPH_COMPILER_DEBUG") {
                print!($($t)*);
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

        after_or_equal_cache: RefCell::default(),
        after_or_equal_cache_inv: RefCell::default(),
    };

    #[cfg(debug_assertions)]
    let mut rng = {
        let seed = rand::random();
        // let seed = 6_449_357_397_300_112_446_u64;
        tracing::trace!(seed);
        rand::rngs::Xoshiro256PlusPlus::seed_from_u64(seed)
    };

    let mut iterations = 0;

    while !this.is_complete() {
        {
            let cache = this.after_or_equal_cache.borrow();
            let mut cache_inv = this.after_or_equal_cache_inv.borrow_mut();
            cache_inv.clear();
            #[expect(clippy::iter_over_hash_type, reason = "t")]
            for &(k, v) in cache.iter() {
                if let InputOrNode::Node(k) = k {
                    cache_inv.insert((InputOrNode::Node(v), k));
                }
            }
        }

        rr_println!("##############################");
        #[derive(Debug, Clone, Copy)]
        struct ConsumeClaim {
            node: NodeRef,
            input_idx: usize,
            is_from_unordered: bool,
        }
        #[derive(Debug, Default, Clone)]
        struct ClaimList {
            borrows: Vec<(NodeRef, usize)>,
            consumes: Vec<ConsumeClaim>,
        }
        // We use OrderMap for deterministic iteration
        let mut claims = OrderMap::<(ResourceRef, InputOrNode), ClaimList>::new();
        for (node, node_data) in graph.nodes() {
            let finished = chain!(&this.resolved_inputs[&node], &this.resolved_borrows[&node]).all(Option::is_some);
            if finished { continue }
            rr_println!("{}", nn!(node));
            for (input_idx, resource) in node_data.consumes().enumerate() {
                if this.resolved_inputs[&node][input_idx].is_some() { continue; }
                let Some(potential_producers) = producers.get(&resource)
                else { panic!("Missing producers for {}", rn!(resource)) };

                rr_print!("\tconsumes({}): ", rn!(resource));

                let producer = potential_producers.iter().filter(|&&producer| {
                    !this.is_after_or_equal(producer, node)
                }).exactly_one();

                let is_mutator = node_data.outputs().contains(&resource);
                let is_unordered = graph.resource_cfg(resource).unordered;
                
                let producer = if is_mutator && is_unordered && let Err(options) = producer {
                    #[cfg(debug_assertions)]
                    { options.choose(&mut rng).ok_or(Either::Right(())).map(|p| (p, true)) }
                    #[cfg(not(debug_assertions))]
                    { let mut options = options; options.next().ok_or(Either::Right(())).map(|p| (p, true)) }
                } else {
                    producer.map(|p| (p, false)).map_err(Either::Left)
                };

                match producer {
                    Ok((&p, is_from_unordered)) => {
                        rr_println!("{}{}", ton!(p), if is_unordered { " (using unordered)" } else { "" });
                        claims.entry((resource, p)).or_default().consumes.push(ConsumeClaim { node, input_idx, is_from_unordered });
                    },
                    Err(Either::Left(options)) => {
                        rr_println!("{}", options.map(|&n| ton!(n)).join(", "));
                    },
                    Err(Either::Right(())) => {
                        rr_println!("No producers for unordered");
                    },
                }
            }
            for (borrow_idx, resource) in node_data.borrows().enumerate() {
                if this.resolved_borrows[&node][borrow_idx].is_some() { continue }
                let Some(potential_producers) = producers_for_borrows.get(&resource)
                else { panic!("Missing producers for {}", rn!(resource)) };

                rr_print!("\tborrows({}): ", rn!(resource));

                let producer = potential_producers.iter().filter(|&&producer| {
                    !this.is_after_or_equal(producer, node)
                }).filter(|&&producer| {
                    this.consumer(ResolvedInput { resource, producer })
                        .is_none_or(|p| !this.is_after_or_equal(InputOrNode::Node(node), p))
                }).exactly_one();

                match producer {
                    Ok(&p) => {
                        rr_println!("{}", ton!(p));
                        claims.entry((resource, p)).or_default().borrows.push((node, borrow_idx));
                    },
                    Err(options) => {
                        rr_println!("{}", options.map(|&n| ton!(n)).join(", "));
                    },
                }
            }
        }

        #[cfg(debug_assertions)]
        let claims = {
            use rand::seq::SliceRandom as _;

            let mut claims = claims.into_iter().collect_vec();
            claims.shuffle(&mut rng);
            claims
        };

        let accept_unordered = claims.iter().all(|(_,claim)| claim.borrows.is_empty() && claim.consumes.iter().all(|c| c.is_from_unordered));

        let mut found_valid = false;
        'claim_loop: for ((claim_resource, claim_producer), claimers) in claims {
            if claimers.borrows.is_empty() {
                let wining_claim = claimers.consumes.iter().exactly_one().ok()
                    .and_then(|p| (!p.is_from_unordered || accept_unordered).then_some(p))
                    .map_or_else(|| {
                        (accept_unordered && claimers.consumes.iter().all(|o| o.is_from_unordered))
                            .then(|| {
                                #[cfg(debug_assertions)]
                                { claimers.consumes.choose(&mut rng).unwrap() }
                                #[cfg(not(debug_assertions))]
                                { &claimers.consumes[0] }
                            })
                    }, Some);
                if let Some(&ConsumeClaim { node, input_idx, is_from_unordered }) = wining_claim {
                    rr_println!("REVOLED {} consumes {} from {}", nn!(node), rn!(claim_resource), ton!(claim_producer));
                    found_valid = true;
                    debug_assert!(this.resolved_inputs[node][input_idx].is_none());
                    this.resolved_inputs[node][input_idx] = Some(ResolvedInput {
                        resource: claim_resource,
                        producer: claim_producer,
                    });
                    producers.get_mut(&claim_resource).unwrap().retain(|p| p != &claim_producer);

                    // Unordered reseources may break the assumption that claims
                    // made at the same time are not 'incompatible'
                    // A claim can change node orderign (as per is_after_or_equal)
                    // and so make other claims invalid.
                    // FIXME: This could be replaced by a more specific "unordered"
                    // claim logic(?)
                    if is_from_unordered {
                        break 'claim_loop;
                    }
                }
                else {
                    tracing::warn!(resource = %rn!(claim_resource), producer = %ton!(claim_producer), claimers = ?claimers.consumes, "Consumer conflict");
                }
            }
            else {
                found_valid = true;
                for (node_id, input_idx) in claimers.borrows {
                    rr_println!("REVOLED {} borrows {} from {}", nn!(node_id), rn!(claim_resource), ton!(claim_producer));
                    debug_assert!(this.resolved_borrows[&node_id][input_idx].is_none());
                    this.resolved_borrows[&node_id][input_idx] = Some(ResolvedInput {
                        resource: claim_resource,
                        producer: claim_producer,
                    });
                }
            }
        }
        if !found_valid {
            return None;
        }

        iterations += 1;
    }

    tracing::trace!(iterations, "Resolved render graph resources");

    Some(ResolvedResources {
        node_count: graph.nodes.len(),
        consumers: this.consumers(),
        borrowers: this.borrowers(),
        explicit_orderings: this.explicit_orderings,
        inputs: this.resolved_inputs.into_iter().map(|p| p.into_iter().map(Option::unwrap).collect()).collect(),
        borrows: this.resolved_borrows.into_iter().map(|p| p.into_iter().map(Option::unwrap).collect()).collect(),
    })
}

#[expect(clippy::single_call_fn, reason = "CompiledGraph construction algorithm")]
fn compute_total_order(graph: &RenderGraph, resolved: &ResolvedResources) -> Box<[NodeRef]> {
    let node_labels = graph.get_simplified_node_labels();
    let resource_labels = graph.get_simplified_resource_labels();
    macro_rules! rn {
        ($t:expr) => {{
            // &graph.resources.with($t.0).unwrap().get().label
            resource_labels[&graph.resource_handle($t)].clone()
        }};
    }
    macro_rules! nn {
        ($n:expr) => {
            node_labels.get(&graph.node_handle($n)).cloned().unwrap_or_default()
        };
    }
    macro_rules! ton {
        ($n:expr) => {
            match $n {
                InputOrNode::Input => "<input>".to_string(),
                InputOrNode::Node(n) => nn!(n),
            }
        };
    }

    tracing::debug!("Computing graph total order");
    let mut successors: IndexMap<HashSet<NodeRef>, NodeRef> = vec![HashSet::new(); graph.nodes.len()].into();
    for (node_idx, input) in chain!(resolved.borrows.enumerated(), resolved.inputs.enumerated()).flat_map(|(node_idx, inputs)| inputs.iter().map(move |input| (node_idx, input))) {
        if let InputOrNode::Node(node_idx2) = &input.producer {
            successors[node_idx2].insert(node_idx);
        }
    }
    for (borrower_node, borrows) in resolved.borrows.enumerated() {
        for borrow in borrows {
            let consumer = resolved.inputs.enumerated().flat_map(|(consumer, inputs)| inputs.iter().map(move |input| (consumer, input)))
                .find(|&(_,p)| p == borrow);
            if let Some((consumer,_)) = consumer {
                println!("{consumer_} is after {borrower_} because {borrower_} borrows {} from {} which is consumed by {consumer_}", rn!(borrow.resource), ton!(borrow.producer), consumer_ = nn!(consumer), borrower_ = nn!(borrower_node));
                successors[&borrower_node].insert(consumer);
            }
        }
    }
    let successors = successors;

    let mut done = IndexMap::<bool, NodeRef>::from(vec![false; graph.nodes.len()]);
    let mut output = Vec::<NodeRef>::new();
    while !done.iter().copied().all(identity) {
        let Some((node, _)) = successors.enumerated().filter(|(i, _)| !done[i]).find(|(_,succ)| !succ.iter().any(|o| !done[o]))
        else {
            println!("Remaining:");
            for (r,_) in graph.nodes() {
                if done[r] { continue }
                println!("\t{}: {}", nn!(r), successors[r].iter().filter(|p| !done[**p]).map(|p| nn!(*p)).join(", "));
            }
            println!();
            panic!("Could not resolve graph ordering");
        };
        println!("push({})", nn!(node));
        done[&node] = true;
        output.push(node);
    }
    output.reverse();

    output.into()
}

#[derive(Default)]
pub struct ComputeResult {
    pub required_inputs: Box<[UncheckedResourceHandle]>,
    pub steps: Box<[UncheckedNodeHandle]>,
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
    pub fn new(graph: &RenderGraph) -> Self {
        let producers = compute_producers(graph);
        assert!(
            producers.iter()
                .filter(|&(&k, _)| graph.is_resource_ref_permanent(k))
                .all(|(_, v)| v.len() == 1),
            "Permanent resources must have exactly one producer"
        );
        let mut resolved = None;
        for i in 0..5usize {
            print!("\x1B[2J\x1B[3J\x1B[H");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            println!("START {i}");
            println!("{}", graph.nodes.len());
            resolved = resolve_resources(graph);
            if resolved.is_some() { break }
            tracing::trace!("Fail");
        }
        let resolved = resolved.expect("Could not resolve render graph resources");
        if let Some(found) = resolved.find_loop() {
            tracing::warn!("FOUND A LOOP");
            let node_labels = graph.get_simplified_node_labels();
            macro_rules! nn {
                ($n:expr) => {
                    node_labels.get(&graph.node_handle($n)).cloned().unwrap_or_default()
                };
            }
            macro_rules! ton {
                ($n:expr) => {
                    match $n {
                        InputOrNode::Input => "<input>".to_string(),
                        InputOrNode::Node(n) => nn!(n),
                    }
                };
            }
            for f in found {
                print!("-> {}", ton!(f));
            }
            println!();
            panic!();
        }
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

    fn mark_dirty_rec(not_dirty: &mut Vec<ResourceRef>, resolved: &ResolvedResources, graph: &RenderGraph, res: ResolvedInput) {
        if graph.is_resource_ref_permanent(res.resource)
            && let Some(idx) = not_dirty.iter().position(|p| p == &res.resource) {
                not_dirty.swap_remove(idx);
            }
        for user in resolved.users(res) {
            for output in graph.node(user).outputs() {
                Self::mark_dirty_rec(not_dirty, resolved, graph, ResolvedInput {
                    resource: output,
                    producer: InputOrNode::Node(user),
                });
            }
        }
    }

    pub fn mark_resource_dirty(&mut self, graph: &RenderGraph, resource: UncheckedResourceHandle) {
        let resource = graph.resource_ref(resource);
        assert!(graph.is_resource_ref_permanent(resource), "Only permanent resources can bbe marked dirty");
        let &[producer] = self.producers.get(&resource).map(Vec::as_slice).unwrap_or_default()
        else { panic!("Permanent resources only have one producer") };
        Self::mark_dirty_rec(&mut self.not_dirty, &self.resolved, graph, ResolvedInput { resource, producer });
        self.not_dirty.sort_unstable_by_key(indexmap::MapIndex::as_usize);
    }

    fn is_dirty(&self, graph: &RenderGraph, res: &ResolvedInput) -> bool {
        if graph.is_resource_ref_permanent(res.resource) {
            !self.not_dirty.contains(&res.resource)
        }
        else {
            true
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn compute(&self, graph: &RenderGraph, resource: UncheckedResourceHandle) -> Arc<ComputeResult> {
        let resource = graph.resource_ref(resource);
        let cached = self.cache.read().get(&self.not_dirty).map(Arc::clone);
        if let Some(cached) = cached {
            return cached;
        }
        tracing::trace!("Cache miss");

        assert!(graph.is_resource_ref_permanent(resource), "Can only compute a permanent resource");
        let &[InputOrNode::Node(producer)] = self.producers.get(&resource).map(Vec::as_slice).unwrap_or_default()
        else { panic!("Can only compute if there is exactly one producer of a resource, and that producer isn't the input"); };
        if !self.is_dirty(graph, &ResolvedInput { resource, producer: InputOrNode::Node(producer) }) {
            return Arc::new(ComputeResult::default());
        }

        let mut required_inputs = HashSet::<ResourceRef>::new();
        let mut done = IndexMap::<bool, NodeRef>::from(vec![false; graph.nodes.len()]);
        let mut needed = IndexMap::<bool, NodeRef>::from(vec![false; graph.nodes.len()]);
        let mut stack = Vec::<NodeRef>::new();
        let mut created: Vec<ResourceRef> = vec![];

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
                        if graph.is_resource_ref_permanent(input.resource) {
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
            required_inputs: required_inputs.into_iter().map(|r| graph.resource_handle(r)).collect(),
            steps: self.total_order.iter().filter(|&p| needed[p]).map(|&node| graph.node_handle(node)).collect(),
            output_new_permanents: created.into(),
        });
        debug_assert!(self.not_dirty.is_sorted_by_key(indexmap::MapIndex::as_usize));
        self.cache.write().insert(self.not_dirty.clone(), Arc::clone(&result));

        result
    }

    pub fn apply_compute_result(&mut self, _graph: &RenderGraph, result: &ComputeResult) {
        self.not_dirty.extend(result.output_new_permanents.iter().copied());
        debug_assert!(self.not_dirty.iter().all_unique());
        self.not_dirty.sort_unstable_by_key(indexmap::MapIndex::as_usize);
    }
}
