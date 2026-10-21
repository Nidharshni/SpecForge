use specforge_common::{SourceSpan, Sym};
use specforge_cycles::Cycle;
use specforge_parser::{EntityId, EntityKind, FieldMap};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

mod degree;
mod link;
mod reach;
mod select;
pub use degree::Degree;
pub use reach::Direction;
pub use select::Selected;

#[derive(Debug, Clone)]
pub struct Node {
    pub id: EntityId,
    pub kind: EntityKind,
    pub title: Option<String>,
    pub fields: FieldMap,
    pub source_span: SourceSpan,
    /// `method` members parsed from the entity body (ports define their
    /// interfaces this way). Empty for kinds that never declare methods.
    pub methods: Vec<specforge_parser::MethodDecl>,
    /// `verify` statements in declaration order (ADR 0094): an entity's own
    /// member, never a field.
    pub obligations: Vec<specforge_parser::VerifyStatement>,
}

impl Node {
    /// A node of `kind` named `id` at `source_span`, with no title, field, method or
    /// obligation.
    pub fn new(id: &str, kind: &str, source_span: SourceSpan) -> Node {
        Node {
            id: EntityId { raw: Sym::new(id) },
            kind: EntityKind {
                raw: Sym::new(kind),
            },
            title: None,
            fields: FieldMap::new(),
            source_span,
            methods: Vec::new(),
            obligations: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub source: Sym,
    pub target: Sym,
    pub label: Sym,
}

/// An entity a reach ([`Graph::reach`], [`Graph::reach_directed`]) reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reached {
    pub id: Sym,
    /// Hops from the root.
    pub depth: usize,
    /// The entity it was first reached from, and the label of the edge
    /// between them; `None` for the root.
    pub via: Option<(Sym, Sym)>,
}

#[derive(Debug, Clone)]
pub struct Graph {
    /// BTreeMap keyed by Sym: iteration is string-sorted (Sym::cmp orders
    /// by content), so reads are deterministic with no sort-at-read
    /// compensation (C5-12).
    nodes: BTreeMap<Sym, Node>,
    edges: Vec<Edge>,
    /// Index: source sym -> indices into `edges`
    source_index: HashMap<Sym, Vec<usize>>,
    /// Index: target sym -> indices into `edges`
    target_index: HashMap<Sym, Vec<usize>>,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            source_index: HashMap::new(),
            target_index: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: Node) {
        self.nodes.insert(node.id.raw, node);
    }

    /// Remove every node `file` contributed; the build re-links every edge
    /// afterwards.
    pub(crate) fn remove_entities_of_file(&mut self, file: Sym) {
        self.nodes.retain(|_, n| n.source_span.file != file);
    }

    /// Insert an edge. Idempotent on the (source, target, label) triple —
    /// duplicate edges corrupted cycle reporting and inflated renderers
    /// (C5-09/C5-02).
    pub fn add_edge(&mut self, edge: Edge) {
        if self.index_of(&edge).is_some() {
            return;
        }
        let idx = self.edges.len();
        self.source_index.entry(edge.source).or_default().push(idx);
        self.target_index.entry(edge.target).or_default().push(idx);
        self.edges.push(edge);
    }

    /// The index of the edge with `edge`'s (source, target, label), if the graph holds it.
    fn index_of(&self, edge: &Edge) -> Option<usize> {
        self.source_index
            .get(&edge.source)?
            .iter()
            .copied()
            .find(|&i| self.edges[i].target == edge.target && self.edges[i].label == edge.label)
    }

    fn clear_edges(&mut self) {
        self.edges.clear();
        self.source_index.clear();
        self.target_index.clear();
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.get(&Sym::new(id))
    }

    pub fn nodes(&self) -> Vec<&Node> {
        let mut nodes: Vec<_> = self.nodes.values().collect();
        nodes.sort_by_key(|n| n.id.raw);
        nodes
    }

    /// Node data, for in-crate passes that rewrite it (field coercion).
    /// Edges are untouched: callers re-resolve references afterwards.
    pub(crate) fn nodes_mut(&mut self) -> impl Iterator<Item = &mut Node> {
        self.nodes.values_mut()
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    pub fn nodes_in_file(&self, file: &str) -> Vec<&Node> {
        self.nodes
            .values()
            .filter(|n| n.source_span.file == file)
            .collect()
    }

    /// Filter nodes by a predicate function.
    /// Returns all nodes for which the predicate returns true.
    pub fn filter_nodes<F>(&self, predicate: F) -> Vec<&Node>
    where
        F: Fn(&Node) -> bool,
    {
        self.nodes.values().filter(|n| predicate(n)).collect()
    }

    /// Get all nodes of a specific entity kind.
    pub fn nodes_by_kind(&self, kind: &str) -> Vec<&Node> {
        self.filter_nodes(|n| n.kind.raw == kind)
    }

    pub fn edges_from(&self, id: &str) -> Vec<&Edge> {
        let sym = Sym::new(id);
        self.source_index
            .get(&sym)
            .map(|indices| indices.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }

    pub fn edges_to(&self, id: &str) -> Vec<&Edge> {
        let sym = Sym::new(id);
        self.target_index
            .get(&sym)
            .map(|indices| indices.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// The neighbours of `id` over edges both ways, each with the label of
    /// the edge to it, in `(id, label)` order.
    fn labelled_neighbors(&self, id: Sym) -> Vec<(Sym, Sym)> {
        let mut result = Vec::new();
        if let Some(indices) = self.source_index.get(&id) {
            result.extend(
                indices
                    .iter()
                    .map(|&i| (self.edges[i].target, self.edges[i].label)),
            );
        }
        if let Some(indices) = self.target_index.get(&id) {
            result.extend(
                indices
                    .iter()
                    .map(|&i| (self.edges[i].source, self.edges[i].label)),
            );
        }
        result.sort_unstable();
        result
    }

    /// Breadth-first from `root_id` over edges both ways, nearest first: the
    /// one traversal the subgraphs and the explore prompt share. Each
    /// node's neighbours are taken in `(id, label)` order, so the order and
    /// each [`Reached::via`] are deterministic. Only nodes of the graph are
    /// reached (an edge to an id no node declares is not walked), and
    /// nothing beyond `max_depth` hops (unbounded when `None`; depth 0 is
    /// the root alone). `None` when `root_id` is not a node.
    pub fn reach(&self, root_id: &str, max_depth: Option<usize>) -> Option<Vec<Reached>> {
        self.breadth_first(root_id, max_depth, |id| self.labelled_neighbors(id))
    }

    /// The edge labels on the path [`Self::reach`] found to `id`, root
    /// first: empty for the root, or for an id it did not reach.
    pub fn reach_path(reached: &[Reached], id: Sym) -> Vec<Sym> {
        let mut labels = Vec::new();
        let mut at = reached.iter().rposition(|r| r.id == id);
        while let Some(index) = at {
            let Some((from, label)) = reached[index].via else {
                break;
            };
            labels.push(label);
            // A node is reached from one reached before it.
            at = reached[..index].iter().rposition(|r| r.id == from);
        }
        labels.reverse();
        labels
    }

    /// The subgraph `ids` induces: those of them that are nodes, and every
    /// edge between two of them, in edge order.
    pub fn induced(&self, ids: impl IntoIterator<Item = Sym>) -> Graph {
        let ids: HashSet<Sym> = ids.into_iter().collect();
        let mut sub = Graph::new();
        for id in &ids {
            if let Some(node) = self.nodes.get(id) {
                sub.add_node(node.clone());
            }
        }
        for edge in &self.edges {
            if ids.contains(&edge.source) && ids.contains(&edge.target) {
                sub.add_edge(*edge);
            }
        }
        sub
    }

    /// Extract the subgraph reachable from `root_id` following edges in both directions.
    /// Returns None if `root_id` is not in the graph.
    pub fn subgraph(&self, root_id: &str) -> Option<Graph> {
        self.reach(root_id, None)
            .map(|reached| self.induced(reached.iter().map(|r| r.id)))
    }

    /// Extract the subgraph reachable from `root_id` within `max_depth` hops (both directions).
    /// Depth 0 returns only the root. Returns None if `root_id` is not in the graph.
    pub fn subgraph_depth(&self, root_id: &str, max_depth: usize) -> Option<Graph> {
        self.reach(root_id, Some(max_depth))
            .map(|reached| self.induced(reached.iter().map(|r| r.id)))
    }
}

impl Graph {
    /// The cycles of the dependency relation (ADR 0204): every edge `a -> b`
    /// says `a` depends on `b`, whatever its label, a written, single or
    /// derived reference alike. Each is a strongly connected set of entities
    /// with a closed walk through all of them (ADR 0196). W061 reads a
    /// narrower relation, the written references (`link`).
    pub fn cycles(&self) -> Vec<Cycle<Sym>> {
        specforge_cycles::cycles(&successors(&self.edges))
    }
}

/// Each source's targets among `edges`: the form `specforge_cycles::cycles`
/// reads. Parallel edges are one.
fn successors<'e>(edges: impl IntoIterator<Item = &'e Edge>) -> BTreeMap<Sym, BTreeSet<Sym>> {
    let mut successors: BTreeMap<Sym, BTreeSet<Sym>> = BTreeMap::new();
    for edge in edges {
        successors
            .entry(edge.source)
            .or_default()
            .insert(edge.target);
    }
    successors
}
