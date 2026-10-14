use specforge_common::{Diagnostic, DiagnosticData, SourceSpan, Sym, codes, find_close_match};
use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue};
use std::collections::{BTreeMap, HashMap, HashSet};

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
}

#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub source: Sym,
    pub target: Sym,
    pub label: Sym,
}

/// An entity [`Graph::reach`] reached.
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
    /// Known bidirectional edge pairs where A->B with label X and B->A with label Y
    /// represent complementary relationships, not real circular dependencies.
    /// Each pair is (label_forward, label_reverse).
    /// Populated by callers (e.g., from extension edge types) rather than hardcoded.
    bidirectional_pairs: Vec<(String, String)>,
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
            bidirectional_pairs: Vec::new(),
        }
    }

    /// Create a new graph with the given bidirectional edge pairs.
    /// These pairs are used to suppress false-positive cycle detection
    /// for complementary relationships (e.g., `invariants`/`enforced_by`).
    pub fn with_bidirectional_pairs(bidirectional_pairs: Vec<(String, String)>) -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            source_index: HashMap::new(),
            target_index: HashMap::new(),
            bidirectional_pairs,
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
        if self
            .edges
            .iter()
            .any(|e| e.source == edge.source && e.target == edge.target && e.label == edge.label)
        {
            return;
        }
        let idx = self.edges.len();
        self.source_index.entry(edge.source).or_default().push(idx);
        self.target_index.entry(edge.target).or_default().push(idx);
        self.edges.push(edge);
    }

    fn clear_edges(&mut self) {
        self.edges.clear();
        self.source_index.clear();
        self.target_index.clear();
    }

    /// The reference-list entries that name an existing entity but have no
    /// edge (source, target, field). Empty after every link.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(crate) fn unlinked_references(&self) -> Vec<(Sym, Sym, Sym)> {
        let mut unlinked = Vec::new();
        for node in self.nodes.values() {
            let source = node.id.raw;
            let edges = self.source_index.get(&source);
            for entry in node.fields.entries() {
                let FieldValue::ReferenceList(refs) = &entry.value else {
                    continue;
                };
                for target in refs {
                    let target = Sym::new(target.as_str());
                    if !self.nodes.contains_key(&target) {
                        continue;
                    }
                    let linked = edges.is_some_and(|indices| {
                        indices.iter().any(|&i| {
                            let edge = &self.edges[i];
                            edge.target == target && edge.label == entry.key
                        })
                    });
                    if !linked {
                        unlinked.push((source, target, entry.key));
                    }
                }
            }
        }
        unlinked
    }

    /// Panic, naming them, when [`Self::unlinked_references`] is not empty:
    /// a SpecForge bug, never a spec error.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(crate) fn assert_linked(&self) {
        let unlinked = self.unlinked_references();
        assert!(
            unlinked.is_empty(),
            "SpecForge bug: references to existing entities without their edge (source, target, field): {}",
            unlinked
                .iter()
                .map(|(source, target, field)| format!("({source}, {target}, {field})"))
                .collect::<Vec<_>>()
                .join(", ")
        );
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
        let root = Sym::new(root_id);
        if !self.nodes.contains_key(&root) {
            return None;
        }
        let mut reached = vec![Reached {
            id: root,
            depth: 0,
            via: None,
        }];
        let mut seen: HashSet<Sym> = HashSet::from([root]);
        let mut next = 0;
        while let Some(&Reached { id, depth, .. }) = reached.get(next) {
            next += 1;
            if max_depth.is_some_and(|max| depth >= max) {
                continue;
            }
            for (neighbor, label) in self.labelled_neighbors(id) {
                if self.nodes.contains_key(&neighbor) && seen.insert(neighbor) {
                    reached.push(Reached {
                        id: neighbor,
                        depth: depth + 1,
                        via: Some((id, label)),
                    });
                }
            }
        }
        Some(reached)
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

    /// The subgraph `reached` induces: its nodes, and every edge between
    /// two of them, in edge order.
    fn induced(&self, reached: &[Reached]) -> Graph {
        let ids: HashSet<Sym> = reached.iter().map(|r| r.id).collect();
        let mut sub = Graph::with_bidirectional_pairs(self.bidirectional_pairs.clone());
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
            .map(|reached| self.induced(&reached))
    }

    /// Extract the subgraph reachable from `root_id` within `max_depth` hops (both directions).
    /// Depth 0 returns only the root. Returns None if `root_id` is not in the graph.
    pub fn subgraph_depth(&self, root_id: &str, max_depth: usize) -> Option<Graph> {
        self.reach(root_id, Some(max_depth))
            .map(|reached| self.induced(&reached))
    }

    /// Resolve reference fields into graph edges and return E003 (and I004)
    /// diagnostics for any unresolved references.
    ///
    /// The graph build's linking step (`GraphBuild`): the one place
    /// references resolve, so a compile and every update agree.
    ///
    /// The method clears all existing edges, then iterates every node's
    /// `ReferenceList` fields plus any `Identifier` fields that match
    /// `single_ref_fields` (a set of `(entity_kind, field_name)` pairs
    /// declaring which Identifier fields are single-reference fields).
    /// For each target that exists in the graph an edge is created; for
    /// each target that does *not* exist a diagnostic is emitted (with a
    /// fuzzy-match suggestion when possible). An unresolved target in a
    /// field listed in `absent_targets` ((kind, field) -> target kind) is
    /// an I004 hint: no loaded extension declares that kind, so the
    /// reference can't resolve until its extension is enabled.
    pub(crate) fn resolve_references_with(
        &mut self,
        single_ref_fields: &HashSet<(String, String)>,
        absent_targets: &HashMap<(String, String), String>,
    ) -> Vec<Diagnostic> {
        self.clear_edges();

        let entity_ids: HashSet<Sym> = self.nodes.keys().copied().collect();

        // Snapshot node data so we can mutate edges while iterating.
        // BTreeMap iteration is string-sorted: diagnostic and edge order do
        // not depend on per-process HashMap seeding (R-6 / C5-12).
        let all_nodes: Vec<(Sym, Sym, FieldMap)> = self
            .nodes
            .values()
            .map(|n| (n.id.raw, n.kind.raw, n.fields.clone()))
            .collect();

        let mut diagnostics = Vec::new();

        for (node_id, node_kind, fields) in &all_nodes {
            for entry in fields.entries() {
                match &entry.value {
                    FieldValue::ReferenceList(refs) => {
                        for target_ref in refs {
                            let target_id = target_ref.as_str();
                            let target_sym = Sym::new(target_id);
                            if entity_ids.contains(&target_sym) {
                                self.add_edge(Edge {
                                    source: *node_id,
                                    target: target_sym,
                                    label: entry.key,
                                });
                            } else if let Some(target_kind) = absent_targets.get(&(
                                node_kind.as_str().to_string(),
                                entry.key.as_str().to_string(),
                            )) {
                                diagnostics.push(
                                    Diagnostic::new(
                                        codes::I004,
                                        format!(
                                            "reference '{}' in field '{}' of '{}' targets kind '{}', which no enabled extension provides",
                                            target_id, entry.key, node_id, target_kind
                                        ),
                                    )
                                    .with_span(target_ref.span.clone())
                                    .with_suggestion(format!(
                                        "enable the extension that declares '{target_kind}' (e.g. `specforge add @specforge/<name>`), or drop the field"
                                    )),
                                );
                            } else {
                                let suggestion = find_close_match(
                                    target_id,
                                    entity_ids.iter().map(|s| s.as_str()),
                                );
                                let mut diag = Diagnostic::new(
                                    codes::E003,
                                    format!(
                                        "unresolved reference '{}' in entity '{}'",
                                        target_id, node_id
                                    ),
                                )
                                .with_span(target_ref.span.clone())
                                .with_data(
                                    DiagnosticData::UnresolvedReference {
                                        target: target_id.to_string(),
                                        entity: node_id.as_str().to_string(),
                                        field: entry.key.as_str().to_string(),
                                        did_you_mean: suggestion.map(str::to_string),
                                    },
                                );
                                if let Some(s) = suggestion {
                                    diag = diag.with_suggestion(format!("did you mean '{}'?", s));
                                }
                                diagnostics.push(diag);
                            }
                        }
                    }
                    FieldValue::Identifier(target_id)
                        if single_ref_fields.contains(&(
                            node_kind.as_str().to_string(),
                            entry.key.as_str().to_string(),
                        )) =>
                    {
                        let target_sym = Sym::new(target_id);
                        if entity_ids.contains(&target_sym) {
                            self.add_edge(Edge {
                                source: *node_id,
                                target: target_sym,
                                label: entry.key,
                            });
                        }
                        // Single refs don't emit E001 — the value might be
                        // a valid enum/identifier rather than a broken ref.
                    }
                    _ => {}
                }
            }
        }

        diagnostics
    }
}

/// Canonical key for a cycle: the sorted member list, so rotations and
/// parallel-edge re-reports collapse to one entry (C5-02).
fn canonical_cycle_key(cycle: &[Sym]) -> Vec<String> {
    let mut members: Vec<String> = cycle.iter().map(|s| s.as_str().to_string()).collect();
    members.sort_unstable();
    members.dedup();
    members
}

impl Graph {
    /// Detect all cycles in the directed edge set using DFS.
    /// Returns a list of cycles, where each cycle is a Vec of node IDs forming the path.
    ///
    /// Two-hop cycles that consist entirely of known bidirectional edge pairs
    /// (e.g., `invariants`/`enforced_by`) are excluded, since they represent
    /// complementary relationships rather than real circular dependencies.
    pub fn detect_cycles(&self) -> Vec<Vec<Sym>> {
        #[derive(Clone, Copy, PartialEq)]
        enum Color {
            White,
            Gray,
            Black,
        }

        let mut color: HashMap<Sym, Color> =
            self.nodes.keys().map(|&k| (k, Color::White)).collect();
        let mut path: Vec<Sym> = Vec::new();
        let mut cycles: Vec<Vec<Sym>> = Vec::new();

        fn dfs(
            node: Sym,
            color: &mut HashMap<Sym, Color>,
            path: &mut Vec<Sym>,
            cycles: &mut Vec<Vec<Sym>>,
            source_index: &HashMap<Sym, Vec<usize>>,
            edges: &[Edge],
        ) {
            color.insert(node, Color::Gray);
            path.push(node);

            if let Some(indices) = source_index.get(&node) {
                for &idx in indices {
                    let target = edges[idx].target;
                    match color.get(&target).copied().unwrap_or(Color::White) {
                        Color::Gray => {
                            // Found a cycle -- extract the cycle path from the stack
                            if let Some(pos) = path.iter().position(|&n| n == target) {
                                let mut cycle: Vec<Sym> = path[pos..].to_vec();
                                cycle.push(target); // close the loop
                                cycles.push(cycle);
                            }
                        }
                        Color::White => {
                            dfs(target, color, path, cycles, source_index, edges);
                        }
                        Color::Black => {}
                    }
                }
            }

            path.pop();
            color.insert(node, Color::Black);
        }

        // C5-12: BTreeMap iteration is string-sorted — which node a cycle
        // is reported from (and thus its rendered rotation) must not depend
        // on HashMap seeding (R-6).
        let node_ids: Vec<Sym> = self.nodes.keys().copied().collect();
        for &node in &node_ids {
            if color.get(&node).copied() == Some(Color::White) {
                dfs(
                    node,
                    &mut color,
                    &mut path,
                    &mut cycles,
                    &self.source_index,
                    &self.edges,
                );
            }
        }

        // C5-08: keep only real cycles. A cycle is complementary — and thus
        // suppressed — when every hop's label belongs to ONE registered
        // (forward, reverse) pair and BOTH directions appear on the path.
        // This generalizes the old 2-hop-only suppression to cycles of any
        // length; a cycle using only forward labels is a genuine cycle.
        cycles.retain(|cycle| !self.is_complementary_cycle(cycle));

        // C5-02: dedupe by canonical member set. Parallel edges used to
        // report the same cycle once per duplicate edge, and the DFS reports
        // each cycle from whichever seed entered it first.
        cycles.sort_by_cached_key(|c| canonical_cycle_key(c));
        cycles.dedup_by(|a, b| canonical_cycle_key(a) == canonical_cycle_key(b));

        cycles
    }

    /// True when `cycle` (closed path, first == last) is composed entirely
    /// of hops whose labels are drawn from a single registered
    /// (forward, reverse) bidirectional pair, with both directions present.
    fn is_complementary_cycle(&self, cycle: &[Sym]) -> bool {
        if cycle.len() < 3 {
            return false;
        }
        let hops = cycle.len() - 1;
        let mut hop_labels: Vec<std::collections::BTreeSet<&str>> = Vec::with_capacity(hops);
        for i in 0..hops {
            let (u, v) = (cycle[i], cycle[i + 1]);
            let mut labels: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
            if let Some(indices) = self.source_index.get(&u) {
                for &idx in indices {
                    if self.edges[idx].target == v {
                        labels.insert(self.edges[idx].label.as_str());
                    }
                }
            }
            if labels.is_empty() {
                return false; // path is not actually connected
            }
            hop_labels.push(labels);
        }

        self.bidirectional_pairs.iter().any(|(fwd, rev)| {
            let all_in_pair = hop_labels
                .iter()
                .all(|ls| ls.iter().all(|l| l == fwd || l == rev));
            let has_fwd = hop_labels.iter().any(|ls| ls.contains(fwd.as_str()));
            let has_rev = hop_labels.iter().any(|ls| ls.contains(rev.as_str()));
            all_in_pair && has_fwd && has_rev
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    const REFERENCING: &str = r#"
behavior alpha "A" {
  contract "first"
  invariants [inv_one]
}
invariant inv_one "Invariant One" {
  contract "must hold"
}
"#;

    /// The graph and diagnostics of `source`, built the way a compile builds it.
    fn built(source: &str) -> (Graph, Vec<Diagnostic>) {
        crate::build_graph(&[specforge_parser::parse(source, "main.spec")])
    }

    #[specforge_test(
        behavior = "detect_dangling_references",
        verify = "a reference to an existing entity without its edge fails the linker's assertion"
    )]
    #[should_panic(expected = "without their edge")]
    fn a_reference_without_its_edge_fails_the_assertion() {
        let (mut graph, _) = built(REFERENCING);
        // What a linker that forgot an edge leaves behind.
        graph.edges.clear();
        graph.source_index.clear();
        graph.target_index.clear();

        graph.assert_linked();
    }

    #[test]
    fn unlinked_references_names_the_missing_edge() {
        let (mut graph, _) = built(REFERENCING);
        graph.clear_edges();

        let unlinked: Vec<(&str, &str, &str)> = graph
            .unlinked_references()
            .iter()
            .map(|(s, t, f)| (s.as_str(), t.as_str(), f.as_str()))
            .collect();

        assert_eq!(unlinked, [("alpha", "inv_one", "invariants")]);
    }

    #[specforge_test(
        behavior = "detect_dangling_references",
        verify = "reference with corresponding graph edge passes"
    )]
    fn a_linked_reference_passes() {
        let (graph, _) = built(REFERENCING);
        graph.assert_linked();
        assert!(graph.unlinked_references().is_empty());
    }

    #[specforge_test(
        behavior = "detect_dangling_references",
        verify = "an empty graph passes the linker's assertion"
    )]
    fn an_empty_graph_passes() {
        let (graph, _) = built("");
        assert_eq!(graph.edge_count(), 0);
        graph.assert_linked();
    }

    #[specforge_test(
        behavior = "detect_dangling_references",
        verify = "Detect Dangling References: dangling reference detection holds — graph_built_fired, resolver_integrity_verified, no_duplicate_diagnostics"
    )]
    fn a_missing_target_is_e003_once() {
        // An unresolved id is the linker's E003, once; it names no existing
        // entity, so the assertion has nothing to say.
        let source = "behavior beta \"B\" { contract \"second\" invariants [missing] }\n";
        let (graph, diagnostics) = built(source);
        assert_eq!(diagnostics.iter().filter(|d| d.code == "E003").count(), 1);
        assert!(graph.unlinked_references().is_empty());
        graph.assert_linked();
    }
}
