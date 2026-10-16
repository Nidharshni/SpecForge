//! The entity snapshot (ADR 0019, ADR 0115): every entity of one built graph as every
//! check after the build reads it, taken once per compile and per session
//! check. It owns the field-text rule, the edge types of its edges (each
//! typed once, by the field that wrote it) and the one obligation rule ([`Standing`]). The registry checks and the rules read
//! its records; the pass input, the custom validators' context and the
//! coverage rule's entities are adapters over it.

mod record;
mod standing;
mod text;
mod wire;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use specforge_graph::Graph;

use crate::compilation::RegistryBuild;
use crate::disk::Disk;
use crate::entity::{EdgeRecord, EntityRecord, RuleInput};

pub use standing::Standing;
pub use text::{field_items, field_text};

/// Every entity of one built graph, read with the registry build it was
/// built with (ADR 0019): taken once per compile and per session check.
#[derive(Debug, Clone, Default)]
pub struct EntitySnapshot {
    /// In id order (the graph's node order).
    records: Vec<EntityRecord>,
    /// Parallel to `records`.
    standings: Vec<Standing>,
    index: HashMap<String, usize>,
    /// In graph order.
    edges: Vec<EdgeRecord>,
    /// In id order.
    declared_types: Vec<String>,
    spec_root: PathBuf,
}

impl EntitySnapshot {
    /// The snapshot of `graph`, read with `registries`; relative file paths
    /// in it resolve against `spec_root`.
    pub fn of(graph: &Graph, registries: &RegistryBuild, spec_root: &Path) -> Self {
        let nodes = graph.nodes();
        let kind_of: HashMap<&str, &str> = nodes
            .iter()
            .map(|n| (n.id.raw.as_str(), n.kind.raw.as_str()))
            .collect();
        let records: Vec<EntityRecord> = nodes
            .iter()
            .map(|node| record::record(graph, &kind_of, node, registries))
            .collect();
        let standings = records
            .iter()
            .map(|record| Standing::of(record, registries))
            .collect();
        let index = records
            .iter()
            .enumerate()
            .map(|(i, record)| (record.id.clone(), i))
            .collect();
        let declared_types = records
            .iter()
            .filter(|record| {
                registries
                    .kinds()
                    .get(&record.kind)
                    .is_some_and(|kind| kind.declared.declares_types)
            })
            .map(|record| record.id.clone())
            .collect();
        let edges = graph
            .edges()
            .iter()
            .map(|e| EdgeRecord {
                source: e.source.as_str().to_string(),
                target: e.target.as_str().to_string(),
                label: e.label.as_str().to_string(),
                edge_type: record::edge_type_of(
                    &kind_of,
                    registries,
                    e.source.as_str(),
                    e.label.as_str(),
                ),
            })
            .collect();
        EntitySnapshot {
            records,
            standings,
            index,
            edges,
            declared_types,
            spec_root: spec_root.to_path_buf(),
        }
    }

    /// Every record, in id order: what the registry checks read.
    pub fn records(&self) -> &[EntityRecord] {
        &self.records
    }

    /// What the rules run over, reading files through `disk`.
    pub fn rule_input<'a>(&'a self, disk: &'a dyn Disk) -> RuleInput<'a> {
        RuleInput {
            entities: &self.records,
            edges: &self.edges,
            spec_root: &self.spec_root,
            disk,
        }
    }

    /// Every entity with its standing, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (&EntityRecord, &Standing)> {
        self.records.iter().zip(&self.standings)
    }

    /// The entity `id`, with its standing.
    pub fn get(&self, id: &str) -> Option<(&EntityRecord, &Standing)> {
        let i = *self.index.get(id)?;
        Some((&self.records[i], &self.standings[i]))
    }

    /// The standing of the entity `id`.
    pub fn standing(&self, id: &str) -> Option<&Standing> {
        self.get(id).map(|(_, standing)| standing)
    }

    /// The kind of the entity `id`.
    pub fn kind_of(&self, id: &str) -> Option<&str> {
        self.get(id).map(|(record, _)| record.kind.as_str())
    }

    /// The graph's edges, in graph order.
    pub fn edges(&self) -> &[EdgeRecord] {
        &self.edges
    }

    /// The ids of the entities whose kind declares types (`declares_types`),
    /// in id order.
    pub fn declared_types(&self) -> &[String] {
        &self.declared_types
    }

    /// What relative paths in it resolve against.
    pub fn spec_root(&self) -> &Path {
        &self.spec_root
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests;
