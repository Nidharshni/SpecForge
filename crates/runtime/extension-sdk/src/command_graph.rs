use crate::{GraphEdge, GraphNode, GraphWire};

/// What a `cmd__<name>` export receives, its graph indexed for lookups
/// ([`CommandGraph`]).
pub type CommandInput = specforge_protocol_types::CommandInput<CommandGraph>;

/// The compiled graph a command reads: its entities sorted by id, its
/// resolved references sorted by (source, target, label).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(from = "GraphWire", into = "GraphWire")]
pub struct CommandGraph {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    by_id: std::collections::HashMap<String, usize>,
    from: std::collections::HashMap<String, Vec<usize>>,
    to: std::collections::HashMap<String, Vec<usize>>,
}

impl From<GraphWire> for CommandGraph {
    fn from(wire: GraphWire) -> Self {
        CommandGraph::new(wire.nodes, wire.edges)
    }
}

impl From<CommandGraph> for GraphWire {
    fn from(graph: CommandGraph) -> Self {
        GraphWire {
            nodes: graph.nodes,
            edges: graph.edges,
        }
    }
}

impl CommandGraph {
    /// A graph of `nodes` and `edges`, indexed for lookups.
    pub fn new(nodes: Vec<GraphNode>, edges: Vec<GraphEdge>) -> Self {
        let mut graph = CommandGraph {
            nodes,
            edges,
            ..Default::default()
        };
        for (i, node) in graph.nodes.iter().enumerate() {
            graph.by_id.insert(node.id.clone(), i);
        }
        for (i, edge) in graph.edges.iter().enumerate() {
            graph.from.entry(edge.source.clone()).or_default().push(i);
            graph.to.entry(edge.target.clone()).or_default().push(i);
        }
        graph
    }

    /// Every entity, sorted by id.
    pub fn nodes(&self) -> &[GraphNode] {
        &self.nodes
    }

    /// Every resolved reference.
    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    /// The entity `id`.
    pub fn node(&self, id: &str) -> Option<&GraphNode> {
        self.by_id.get(id).map(|&i| &self.nodes[i])
    }

    /// The entities of `kind`, sorted by id.
    pub fn nodes_of_kind<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a GraphNode> + 'a {
        self.nodes.iter().filter(move |n| n.kind == kind)
    }

    /// The references out of `id`.
    pub fn edges_from(&self, id: &str) -> Vec<&GraphEdge> {
        self.indexed(&self.from, id)
    }

    /// The references into `id`.
    pub fn edges_to(&self, id: &str) -> Vec<&GraphEdge> {
        self.indexed(&self.to, id)
    }

    fn indexed(
        &self,
        index: &std::collections::HashMap<String, Vec<usize>>,
        id: &str,
    ) -> Vec<&GraphEdge> {
        index
            .get(id)
            .map(|is| is.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }
}
