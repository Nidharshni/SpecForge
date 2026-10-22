//! What changed between two graphs: the one [`GraphDelta`] a session
//! reports for each update, watch prints and MCP notifies.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Graph, Node};
use serde::Serialize;
use serde_json::Value;
use specforge_common::Sym;
use specforge_common::shape::Shape;

/// What changed between two graphs. Node lists are sorted by ID, edge
/// lists by (source, target, label). Source positions are ignored: an
/// entity that only moved is not modified. Watch prints it under
/// `--format json` and MCP sends it as `specforge/graphChanged`'s params,
/// whole (`GraphDelta` in spec/types/graph.spec).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Shape)]
pub struct GraphDelta {
    pub added_nodes: Vec<NodeChange>,
    pub removed_nodes: Vec<NodeChange>,
    /// Nodes in both graphs whose fields, kind,
    /// title, methods, obligations or outgoing edges differ.
    pub modified_nodes: Vec<ModifiedNodeChange>,
    pub added_edges: Vec<EdgeChange>,
    pub removed_edges: Vec<EdgeChange>,
    /// The files of the added, removed and modified nodes (sorted).
    pub affected_files: Vec<String>,
}

/// A node added or removed, where it is declared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
#[shape(define)]
pub struct NodeChange {
    pub id: String,
    pub kind: String,
    pub file: String,
    pub line: usize,
}

/// A node whose content changed, where it is declared now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct ModifiedNodeChange {
    pub id: String,
    /// The fields whose values differ, by name (sorted). A field is named
    /// here whatever its name, `kind` and `methods` included.
    pub changed_fields: Vec<String>,
    /// What else differs (sorted, each once): its kind, its title, its
    /// methods, its obligations, its outgoing edges.
    pub changed_aspects: Vec<NodeAspect>,
    pub file: String,
    pub line: usize,
}

/// What a node is besides its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum NodeAspect {
    Kind,
    Title,
    Methods,
    Obligations,
    Edges,
}

/// One edge in a [`GraphDelta`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Shape)]
#[shape(define)]
pub struct EdgeChange {
    pub source: String,
    pub target: String,
    pub label: String,
}

impl GraphDelta {
    pub fn is_empty(&self) -> bool {
        self.added_nodes.is_empty()
            && self.removed_nodes.is_empty()
            && self.modified_nodes.is_empty()
            && self.added_edges.is_empty()
            && self.removed_edges.is_empty()
    }

    /// Whether applying this delta to `old`'s node IDs and edges yields
    /// exactly `new`'s, and every node it names as modified is in both.
    /// The error names the nodes or edges that disagree.
    pub fn applies(&self, old: &Graph, new: &Graph) -> Result<(), String> {
        let ids = |g: &Graph| -> BTreeSet<String> {
            g.nodes().iter().map(|n| n.id.raw.to_string()).collect()
        };
        let mut nodes = ids(old);
        for change in &self.removed_nodes {
            nodes.remove(&change.id);
        }
        for change in &self.added_nodes {
            nodes.insert(change.id.clone());
        }
        let expected = ids(new);
        if nodes != expected {
            let missing: Vec<&String> = expected.difference(&nodes).collect();
            let unexpected: Vec<&String> = nodes.difference(&expected).collect();
            return Err(format!(
                "node mismatch after applying the delta: missing {missing:?}, unexpected {unexpected:?}"
            ));
        }
        if let Some(change) = self
            .modified_nodes
            .iter()
            .find(|c| old.node(&c.id).is_none() || new.node(&c.id).is_none())
        {
            return Err(format!(
                "modified node '{}' is not in both graphs",
                change.id
            ));
        }

        let edges = |g: &Graph| -> BTreeSet<EdgeChange> {
            g.edges()
                .iter()
                .map(|e| edge_change(&(e.source, e.target, e.label)))
                .collect()
        };
        let mut applied = edges(old);
        for edge in &self.removed_edges {
            applied.remove(edge);
        }
        applied.extend(self.added_edges.iter().cloned());
        let expected = edges(new);
        if applied != expected {
            let show = |e: &EdgeChange| format!("{} -{}-> {}", e.source, e.label, e.target);
            let missing: Vec<String> = expected.difference(&applied).map(show).collect();
            let unexpected: Vec<String> = applied.difference(&expected).map(show).collect();
            return Err(format!(
                "edge mismatch after applying the delta: missing {missing:?}, unexpected {unexpected:?}"
            ));
        }
        Ok(())
    }
}

/// An edge as (source, target, label).
pub(crate) type EdgeKey = (Sym, Sym, Sym);

pub(crate) fn edge_keys(graph: &Graph) -> BTreeSet<EdgeKey> {
    graph
        .edges()
        .iter()
        .map(|e| (e.source, e.target, e.label))
        .collect()
}

/// The delta from `old` to `new`, comparing every node.
pub fn compute_graph_delta(old: &Graph, new: &Graph) -> GraphDelta {
    let ids: BTreeSet<Sym> = old
        .nodes()
        .into_iter()
        .chain(new.nodes())
        .map(|n| n.id.raw)
        .collect();
    let old_nodes = old.nodes().into_iter().map(|n| (n.id.raw, n)).collect();
    diff(&ids, &old_nodes, &edge_keys(old), new)
}

/// The delta from `old` to `new` where only the sources `files` changed: what [`compute_graph_delta`] answers for
/// such a pair, read from those files' nodes and the edges that touch them through the graphs' edge indexes. An
/// edge neither of whose ends is declared in `files`, before or after, cannot change: a node's content comes from
/// its own file and an edge resolves to an ID (the invariant `GraphBuild::apply`'s restricted delta rests on, and
/// its verification checks). A file left out hides its changes.
pub fn compute_graph_delta_over<'f>(
    old: &Graph,
    new: &Graph,
    files: impl IntoIterator<Item = &'f str>,
) -> GraphDelta {
    let mut ids = BTreeSet::new();
    for file in files {
        for graph in [old, new] {
            ids.extend(graph.nodes_in_file(file).into_iter().map(|n| n.id.raw));
        }
    }
    let touching = |graph: &Graph| -> BTreeSet<EdgeKey> {
        ids.iter()
            .flat_map(|id| {
                let (from, to) = (graph.edges_from(id.as_str()), graph.edges_to(id.as_str()));
                from.into_iter().chain(to)
            })
            .map(|e| (e.source, e.target, e.label))
            .collect()
    };
    let old_nodes = ids
        .iter()
        .filter_map(|id| Some((*id, old.node(id.as_str())?)))
        .collect();
    diff_edges(&ids, &old_nodes, &touching(old), &touching(new), new)
}

/// The nodes among `ids` that are in both graphs and whose declared place
/// (file, start line) differs, where each is now, sorted by ID. Only these
/// IDs can have moved: a node outside the changed files keeps its span.
pub(crate) fn moved(
    ids: &BTreeSet<Sym>,
    old_nodes: &BTreeMap<Sym, &Node>,
    new: &Graph,
) -> Vec<NodeChange> {
    ids.iter()
        .filter_map(|id| {
            let (old, node) = (old_nodes.get(id)?, new.node(id.as_str())?);
            let (was, is) = (&old.source_span, &node.source_span);
            (was.file != is.file || was.start_line != is.start_line)
                .then(|| node_change(node, &mut BTreeSet::new()))
        })
        .collect()
}

/// Every node in both graphs whose declared place differs ([`moved`] over
/// every ID): what replacing one graph with another moved.
pub fn moved_nodes(old: &Graph, new: &Graph) -> Vec<NodeChange> {
    let ids: BTreeSet<Sym> = old.nodes().into_iter().map(|n| n.id.raw).collect();
    let old_nodes = old.nodes().into_iter().map(|n| (n.id.raw, n)).collect();
    moved(&ids, &old_nodes, new)
}

/// The delta from a graph to `new`, where only the nodes with an ID in
/// `ids` may have been replaced: `old_nodes` holds what the old graph had
/// under those IDs and `old_edges` all its edges. Every other node is
/// unchanged, so it can only be modified through its outgoing edges.
pub(crate) fn diff(
    ids: &BTreeSet<Sym>,
    old_nodes: &BTreeMap<Sym, &Node>,
    old_edges: &BTreeSet<EdgeKey>,
    new: &Graph,
) -> GraphDelta {
    diff_edges(ids, old_nodes, old_edges, &edge_keys(new), new)
}

/// [`diff`] over given edge sets: `old_edges` and `new_edges` need hold only the edges that touch `ids`.
fn diff_edges(
    ids: &BTreeSet<Sym>,
    old_nodes: &BTreeMap<Sym, &Node>,
    old_edges: &BTreeSet<EdgeKey>,
    new_edges: &BTreeSet<EdgeKey>,
    new: &Graph,
) -> GraphDelta {
    let old_out = outgoing(old_edges);
    let new_out = outgoing(new_edges);
    let no_edges = BTreeSet::new();
    let out = |map: &BTreeMap<Sym, BTreeSet<(Sym, Sym)>>, id: &Sym| -> BTreeSet<(Sym, Sym)> {
        map.get(id).unwrap_or(&no_edges).clone()
    };

    let mut delta = GraphDelta::default();
    let mut affected_files = BTreeSet::new();
    for id in ids {
        match (old_nodes.get(id), new.node(id.as_str())) {
            (None, Some(node)) => delta
                .added_nodes
                .push(node_change(node, &mut affected_files)),
            (Some(node), None) => {
                delta
                    .removed_nodes
                    .push(node_change(node, &mut affected_files));
            }
            (Some(old), Some(node)) => {
                let (fields, aspects) = changes(old, &out(&old_out, id), node, &out(&new_out, id));
                if !fields.is_empty() || !aspects.is_empty() {
                    delta
                        .modified_nodes
                        .push(modified(node, fields, aspects, &mut affected_files));
                }
            }
            (None, None) => {}
        }
    }
    // Outside `ids`, a node is the one the old graph had: only its
    // outgoing edges can differ.
    let sources: BTreeSet<Sym> = old_edges
        .symmetric_difference(new_edges)
        .map(|(source, _, _)| *source)
        .filter(|source| !ids.contains(source))
        .collect();
    for source in sources {
        if let Some(node) = new.node(source.as_str())
            && out(&old_out, &source) != out(&new_out, &source)
        {
            delta.modified_nodes.push(modified(
                node,
                Vec::new(),
                vec![NodeAspect::Edges],
                &mut affected_files,
            ));
        }
    }
    delta.modified_nodes.sort_by(|a, b| a.id.cmp(&b.id));

    let mut added: Vec<EdgeChange> = new_edges.difference(old_edges).map(edge_change).collect();
    let mut removed: Vec<EdgeChange> = old_edges.difference(new_edges).map(edge_change).collect();
    added.sort();
    removed.sort();
    delta.added_edges = added;
    delta.removed_edges = removed;
    delta.affected_files = affected_files.into_iter().collect();
    delta
}

fn outgoing(edges: &BTreeSet<EdgeKey>) -> BTreeMap<Sym, BTreeSet<(Sym, Sym)>> {
    let mut map: BTreeMap<Sym, BTreeSet<(Sym, Sym)>> = BTreeMap::new();
    for (source, target, label) in edges {
        map.entry(*source).or_default().insert((*label, *target));
    }
    map
}

fn edge_change((source, target, label): &EdgeKey) -> EdgeChange {
    EdgeChange {
        source: source.to_string(),
        target: target.to_string(),
        label: label.to_string(),
    }
}

fn node_change(node: &Node, files: &mut BTreeSet<String>) -> NodeChange {
    files.insert(node.source_span.file.to_string());
    NodeChange {
        id: node.id.raw.to_string(),
        kind: node.kind.raw.to_string(),
        file: node.source_span.file.to_string(),
        line: node.source_span.start_line,
    }
}

fn modified(
    node: &Node,
    fields: Vec<String>,
    aspects: Vec<NodeAspect>,
    files: &mut BTreeSet<String>,
) -> ModifiedNodeChange {
    files.insert(node.source_span.file.to_string());
    ModifiedNodeChange {
        id: node.id.raw.to_string(),
        changed_fields: fields,
        changed_aspects: aspects,
        file: node.source_span.file.to_string(),
        line: node.source_span.start_line,
    }
}

/// What differs between two versions of a node, its source positions
/// ignored: the fields whose values differ by name, and apart from them
/// the aspects (each sorted).
fn changes(
    old: &Node,
    old_out: &BTreeSet<(Sym, Sym)>,
    new: &Node,
    new_out: &BTreeSet<(Sym, Sym)>,
) -> (Vec<String>, Vec<NodeAspect>) {
    let (old_fields, new_fields) = (field_contents(old), field_contents(new));
    let fields: BTreeSet<String> = old_fields
        .keys()
        .chain(new_fields.keys())
        .filter(|key| old_fields.get(*key) != new_fields.get(*key))
        .cloned()
        .collect();
    let mut aspects = BTreeSet::new();
    if old.kind.raw != new.kind.raw {
        aspects.insert(NodeAspect::Kind);
    }
    if old.title != new.title {
        aspects.insert(NodeAspect::Title);
    }
    if methods(old) != methods(new) {
        aspects.insert(NodeAspect::Methods);
    }
    if old.obligations != new.obligations {
        aspects.insert(NodeAspect::Obligations);
    }
    if old_out != new_out {
        aspects.insert(NodeAspect::Edges);
    }
    (fields.into_iter().collect(), aspects.into_iter().collect())
}

/// Each field's value as every output writes it: the written form has no positions.
fn field_contents(node: &Node) -> BTreeMap<String, Value> {
    let mut fields: BTreeMap<String, Value> = BTreeMap::new();
    for entry in node.fields.entries() {
        let value = serde_json::json!({
            "value": Value::from(crate::field_form::write(&entry.value)),
            "annotations": serde_json::to_value(&entry.annotations).unwrap_or_default(),
        });
        fields.entry(entry.key.to_string()).or_insert(value);
    }
    fields
}

fn methods(node: &Node) -> Vec<Value> {
    node.methods
        .iter()
        .map(|m| {
            serde_json::json!({
                "name": m.name,
                "params": serde_json::to_value(&m.params).unwrap_or_default(),
                "returns": m.returns,
            })
        })
        .collect()
}
