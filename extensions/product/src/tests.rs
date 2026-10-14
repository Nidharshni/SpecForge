use specforge_extension_sdk::prelude::{CommandGraph, GraphEdge, GraphNode};

mod commands;
mod coverage_matrices;
mod dates_and_effort;
mod dependency_graphs;
mod feature_dependents;
mod host;
mod journey_coverage;
mod lifecycle_passes;
mod lists;
mod milestone_completion;
mod persona_channel_features;
mod project_wide;
mod queries;
mod registration_spec;
mod rollups;
mod shared_contract;
mod term_analytics;
mod traceability;

/// A graph built entity by entity.
#[derive(Default)]
struct G {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

impl G {
    fn node(mut self, id: &str, kind: &str, fields: &[(&str, &str)]) -> Self {
        self.nodes.push(GraphNode {
            id: id.to_string(),
            kind: kind.to_string(),
            title: Some(id.to_string()),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                .collect(),
        });
        self
    }

    fn n(self, id: &str, kind: &str) -> Self {
        self.node(id, kind, &[])
    }

    fn edge(mut self, source: &str, target: &str, label: &str) -> Self {
        self.edges.push(GraphEdge {
            source: source.to_string(),
            target: target.to_string(),
            label: label.to_string(),
        });
        self
    }

    fn build(self) -> CommandGraph {
        CommandGraph::new(self.nodes, self.edges)
    }
}
