use std::collections::{HashMap, HashSet, VecDeque};

use super::{FieldLevel, ModelIntermediate, ModelOptions, ModelRelationship};

impl ModelIntermediate {
    /// The entities `options` selects: those of `options.extension`, among
    /// `options.kinds`, within `options.root`'s reach. Relationships between
    /// two kept entities stay, and the extensions are recounted
    /// ([`ModelIntermediate::recount`]).
    pub(super) fn selected(self, options: &ModelOptions) -> Self {
        let has_filter =
            options.extension.is_some() || !options.kinds.is_empty() || options.root.is_some();

        // No filter applied: the model as it is.
        if !has_filter {
            return self;
        }

        let mut keep: HashSet<&str> = self.entities.iter().map(|e| e.name.as_str()).collect();

        // Extension filter
        if let Some(ref ext) = options.extension {
            keep.retain(|name| {
                self.entities
                    .iter()
                    .any(|e| e.name == *name && e.extension == *ext)
            });
        }

        // Kind filter
        if !options.kinds.is_empty() {
            let kind_set: HashSet<&str> = options.kinds.iter().map(String::as_str).collect();
            keep.retain(|name| kind_set.contains(name));
        }

        // Root + depth: BFS on kind-level adjacency graph
        if let Some(ref root) = options.root {
            let depth = root.depth.unwrap_or(usize::MAX);
            let reachable = bfs_reachable(&root.kind, depth, &self.relationships);
            keep.retain(|name| reachable.contains(*name));
        }

        let keep: HashSet<String> = keep.into_iter().map(str::to_string).collect();
        let ModelIntermediate {
            schema_version,
            extensions,
            mut entities,
            mut relationships,
            edge_type_owners,
        } = self;

        // Filter entities
        entities.retain(|e| keep.contains(&e.name));

        // Prune relationships where either endpoint is filtered out
        relationships.retain(|r| keep.contains(&r.source) && keep.contains(&r.target));

        ModelIntermediate {
            schema_version,
            extensions,
            entities,
            relationships,
            edge_type_owners,
        }
        .recount()
    }

    /// Each entity listing the fields `level` names.
    pub(super) fn with_fields(mut self, level: FieldLevel) -> Self {
        for entity in &mut self.entities {
            match level {
                FieldLevel::None => entity.fields.clear(),
                FieldLevel::Keys => entity
                    .fields
                    .retain(|f| f.is_primary_key || f.required || f.field_type.is_reference()),
                FieldLevel::All => {}
            }
        }
        self
    }
}

fn bfs_reachable(
    root: &str,
    max_depth: usize,
    relationships: &[ModelRelationship],
) -> HashSet<String> {
    // Build undirected adjacency at the kind level
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for rel in relationships {
        adj.entry(rel.source.as_str())
            .or_default()
            .push(rel.target.as_str());
        adj.entry(rel.target.as_str())
            .or_default()
            .push(rel.source.as_str());
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(&str, usize)> = VecDeque::new();

    visited.insert(root.to_string());
    queue.push_back((root, 0));

    while let Some((node, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        if let Some(neighbors) = adj.get(node) {
            for &neighbor in neighbors {
                if !visited.contains(neighbor) {
                    visited.insert(neighbor.to_string());
                    queue.push_back((neighbor, depth + 1));
                }
            }
        }
    }

    visited
}
