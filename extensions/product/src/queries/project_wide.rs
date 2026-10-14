use super::{pct, text, PRODUCT_KINDS};
use crate::vocabulary;
use serde::Serialize;
use specforge_extension_sdk::prelude::CommandGraph;
use std::collections::BTreeMap;

/// `BulkStatusPayload`: one entry per lifecycle kind with entities.
#[derive(Debug, Serialize)]
pub struct BulkStatus {
    pub kinds: Vec<KindStatusCounts>,
}

#[derive(Debug, Serialize)]
pub struct KindStatusCounts {
    pub kind: String,
    pub total: usize,
    pub by_status: Vec<StatusCount>,
}

#[derive(Debug, Serialize)]
pub struct StatusCount {
    pub status: String,
    pub count: usize,
}

/// Per lifecycle kind present in the graph, how many entities have each
/// status (`(none)` for those without one).
pub fn bulk_status(graph: &CommandGraph) -> BulkStatus {
    let kinds = vocabulary::LIFECYCLES
        .iter()
        .filter_map(|&(kind, _)| {
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            let mut total = 0;
            for node in graph.nodes_of_kind(kind) {
                total += 1;
                let status = text(node, "status").unwrap_or_else(|| "(none)".to_string());
                *counts.entry(status).or_insert(0) += 1;
            }
            (total > 0).then(|| KindStatusCounts {
                kind: kind.to_string(),
                total,
                by_status: counts
                    .into_iter()
                    .map(|(status, count)| StatusCount { status, count })
                    .collect(),
            })
        })
        .collect();
    BulkStatus { kinds }
}

/// `HealthPayload`: the 0 to 100 project health score and its counts.
#[derive(Debug, Serialize)]
pub struct HealthPayload {
    pub score: HealthScore,
    pub entity_counts: Vec<KindCount>,
    pub unreferenced_counts: Vec<KindUnreferencedCount>,
    pub completeness: HealthCompleteness,
}

#[derive(Debug, Serialize)]
pub struct HealthScore {
    pub overall: f64,
    pub coverage: f64,
    pub connectivity: f64,
    pub completeness: f64,
}

#[derive(Debug, Serialize)]
pub struct KindCount {
    pub kind: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct KindUnreferencedCount {
    pub kind: String,
    pub unreferenced: usize,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct HealthCompleteness {
    pub features_with_status: usize,
    pub features_total: usize,
    pub milestones_with_features: usize,
    pub milestones_total: usize,
}

/// A 0-100 score averaging coverage (product entities something references),
/// connectivity (edge density) and completeness (features with a status,
/// milestones with references).
pub fn project_health(graph: &CommandGraph) -> HealthPayload {
    let mut entity_counts = Vec::new();
    let mut unreferenced_counts = Vec::new();
    let (mut total_entities, mut total_unreferenced) = (0usize, 0usize);
    for &kind in PRODUCT_KINDS {
        let count = graph.nodes_of_kind(kind).count();
        let unreferenced = graph
            .nodes_of_kind(kind)
            .filter(|n| graph.edges_to(&n.id).is_empty())
            .count();
        entity_counts.push(KindCount {
            kind: kind.to_string(),
            count,
        });
        if count > 0 {
            unreferenced_counts.push(KindUnreferencedCount {
                kind: kind.to_string(),
                unreferenced,
                total: count,
            });
        }
        total_entities += count;
        total_unreferenced += unreferenced;
    }

    let coverage = if total_entities > 0 {
        pct(total_entities - total_unreferenced, total_entities)
    } else {
        100.0
    };
    let connectivity = if total_entities > 1 {
        let max_edges = total_entities * (total_entities - 1);
        (graph.edges().len() as f64 / max_edges as f64).min(1.0) * 100.0
    } else {
        100.0
    };

    let features_total = graph.nodes_of_kind("feature").count();
    let features_with_status = graph
        .nodes_of_kind("feature")
        .filter(|n| n.has_field("status"))
        .count();
    let milestones_total = graph.nodes_of_kind("milestone").count();
    let milestones_with_features = graph
        .nodes_of_kind("milestone")
        .filter(|n| !graph.edges_from(&n.id).is_empty())
        .count();
    let completeness = if features_total + milestones_total > 0 {
        pct(
            features_with_status + milestones_with_features,
            features_total + milestones_total,
        )
    } else {
        100.0
    };

    HealthPayload {
        score: HealthScore {
            overall: (coverage + connectivity + completeness) / 3.0,
            coverage,
            connectivity,
            completeness,
        },
        entity_counts,
        unreferenced_counts,
        completeness: HealthCompleteness {
            features_with_status,
            features_total,
            milestones_with_features,
            milestones_total,
        },
    }
}
