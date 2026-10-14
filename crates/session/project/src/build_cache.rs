//! The build cache: the explicit, opt-in record of one build's entity
//! lifecycle states (`specforge-cache.json` at the project root), so
//! history rules such as status transitions compare against a declared
//! input, never hidden state. An entity is recorded when its kind declares
//! a lifecycle field (`lifecycle_field`, ADR 0009); the cache names no
//! field itself. `specforge check --cache` writes it, through the check
//! operation (`specforge_ops::check`), only when the check passes and no
//! other surface does (behavior `write_build_cache`); every compile that
//! runs a check-phase pass reads it and hands it to the passes as
//! `previous` (behavior `read_build_cache`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specforge_common::{Diagnostic, codes};
use specforge_graph::Graph;
use specforge_parser::FieldValue;
use specforge_registry::KindRegistry;

/// The cache's file name, beside `specforge.json`.
pub const BUILD_CACHE_FILE: &str = "specforge-cache.json";

/// The format this SpecForge writes and reads.
pub const BUILD_CACHE_FORMAT: u32 = 1;

/// One build's lifecycle states:
/// `{"format": 1, "statuses": {"<entity id>": {"kind", "status"}}}`,
/// entities sorted by id. `status` is the format's name for the recorded
/// state, whatever the kind's lifecycle field is called.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildCache {
    pub format: u32,
    pub statuses: BTreeMap<String, CachedStatus>,
}

/// An entity's kind and lifecycle state in the build that wrote the cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedStatus {
    pub kind: String,
    pub status: String,
}

/// The cache as a check-phase pass receives it (`PassInput::previous`).
impl From<&BuildCache> for specforge_protocol_types::PassBuildCache {
    fn from(cache: &BuildCache) -> Self {
        specforge_protocol_types::PassBuildCache {
            statuses: cache
                .statuses
                .iter()
                .map(|(id, cached)| {
                    (
                        id.clone(),
                        specforge_protocol_types::PassCachedStatus {
                            kind: cached.kind.clone(),
                            status: cached.status.clone(),
                        },
                    )
                })
                .collect(),
        }
    }
}

impl BuildCache {
    /// The lifecycle states of `graph`: every entity whose kind declares a
    /// lifecycle field in `kinds` and that gives it a text value.
    pub fn of(graph: &Graph, kinds: &KindRegistry) -> Self {
        let statuses = graph
            .nodes()
            .into_iter()
            .filter_map(|node| {
                let field = kinds
                    .get(node.kind.raw.as_str())?
                    .lifecycle_field
                    .as_ref()?;
                let status = match node.fields.get(field)? {
                    FieldValue::Identifier(s) | FieldValue::String(s) => s.clone(),
                    _ => return None,
                };
                Some((
                    node.id.raw.to_string(),
                    CachedStatus {
                        kind: node.kind.raw.to_string(),
                        status,
                    },
                ))
            })
            .collect();
        BuildCache {
            format: BUILD_CACHE_FORMAT,
            statuses,
        }
    }

    /// The cache file's path in the project at `root`.
    pub fn path(root: &Path) -> PathBuf {
        root.join(BUILD_CACHE_FILE)
    }

    /// The file's text: pretty-printed JSON with a final newline, the same
    /// bytes for the same statuses.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Read the cache of the project at `root`: `Ok(None)` without the
    /// file; W144 when it cannot be read, does not parse, or declares
    /// another format.
    pub fn read(root: &Path) -> Result<Option<Self>, Diagnostic> {
        let path = Self::path(root);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(invalid_cache(&format!("cannot be read: {e}"))),
        };
        let cache: BuildCache = serde_json::from_str(&text)
            .map_err(|e| invalid_cache(&format!("does not parse: {e}")))?;
        if cache.format != BUILD_CACHE_FORMAT {
            return Err(invalid_cache(&format!(
                "declares format {}, but this SpecForge reads format {BUILD_CACHE_FORMAT}",
                cache.format
            )));
        }
        Ok(Some(cache))
    }

    /// Write the cache to the project at `root`, replacing the file
    /// atomically (written beside it, then renamed).
    pub fn write(&self, root: &Path) -> std::io::Result<()> {
        let path = Self::path(root);
        let staging = root.join(format!(".{BUILD_CACHE_FILE}.tmp"));
        std::fs::write(&staging, self.to_json())?;
        std::fs::rename(&staging, &path).inspect_err(|_| {
            let _ = std::fs::remove_file(&staging);
        })
    }
}

fn invalid_cache(problem: &str) -> Diagnostic {
    Diagnostic::new(
        codes::W144,
        format!("build cache '{BUILD_CACHE_FILE}' {problem}; status history is ignored"),
    )
    .with_suggestion(format!(
        "rewrite it with `specforge check --cache`, or delete {BUILD_CACHE_FILE}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::{SourceSpan, Sym};
    use specforge_graph::Node;
    use specforge_parser::{EntityId, EntityKind, FieldMap};
    use specforge_registry::KindRegistryEntry;
    use specforge_test_macros::test as specforge_test;

    fn kind(name: &str, lifecycle_field: Option<&str>) -> KindRegistryEntry {
        KindRegistryEntry {
            kind_name: name.to_string(),
            source_extension: "@test/ext".to_string(),
            testable: false,
            supports_verify: false,
            allowed_verify_kinds: vec![],
            lifecycle_field: lifecycle_field.map(str::to_string),
            ..Default::default()
        }
    }

    fn node(id: &str, kind: &str, fields: &[(&str, &str)]) -> Node {
        let mut map = FieldMap::new();
        for (key, value) in fields {
            map.push(Sym::new(key), FieldValue::Identifier(value.to_string()));
        }
        Node {
            id: EntityId { raw: Sym::new(id) },
            kind: EntityKind {
                raw: Sym::new(kind),
            },
            title: None,
            fields: map,
            source_span: SourceSpan {
                file: Sym::new("a.spec"),
                start_line: 1,
                start_col: 0,
                end_line: 1,
                end_col: 0,
            },
            methods: Vec::new(),
        }
    }

    #[specforge_test(
        behavior = "write_build_cache",
        verify = "a kind without a lifecycle field is not recorded"
    )]
    fn only_kinds_declaring_a_lifecycle_field_are_recorded() {
        let mut kinds = KindRegistry::new();
        kinds.register(kind("task", Some("stage")));
        kinds.register(kind("note", None));
        let mut graph = Graph::new();
        // The declared field, whatever its name, is recorded under `status`.
        graph.add_node(node("ship", "task", &[("stage", "doing"), ("status", "x")]));
        // A `status` on a kind that declares no lifecycle field is not.
        graph.add_node(node("memo", "note", &[("status", "draft")]));
        // Nor is an entity of a declaring kind that leaves the field unset.
        graph.add_node(node("idle", "task", &[("status", "x")]));

        let cache = BuildCache::of(&graph, &kinds);
        assert_eq!(
            cache.statuses.into_iter().collect::<Vec<_>>(),
            vec![(
                "ship".to_string(),
                CachedStatus {
                    kind: "task".to_string(),
                    status: "doing".to_string(),
                }
            )]
        );
    }
}
