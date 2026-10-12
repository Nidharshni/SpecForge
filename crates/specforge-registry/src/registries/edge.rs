use std::collections::{BTreeMap, HashMap};

use specforge_protocol_types::EdgeTypeDescriptor;

/// A field that writes an edge type: an entity of `kind` writes, through
/// `field`, edges of the type, each to an entity of `target_kind` when the
/// field declares one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeWriter {
    /// The extension the field is the kind's through (for an enhancement
    /// field, the owner the enhancement names).
    pub extension: String,
    /// The keyword of the kind the field is registered on.
    pub kind: String,
    pub field: String,
    pub target_kind: Option<String>,
}

/// One edge type, resolved once by the registry build: what declares it,
/// every field that writes it and the kinds at its ends. The rule set, the
/// entity snapshot, the schema and trace read it; none derives it again
/// (ADR 0060).
#[derive(Debug, Clone)]
pub struct EdgeRegistryEntry {
    label: String,
    source_extension: String,
    declared: Option<EdgeTypeDescriptor>,
    writers: Vec<EdgeWriter>,
    sources: Vec<String>,
    targets: Vec<String>,
}

impl EdgeRegistryEntry {
    /// The edge type `label`, as the registry build resolved it: its ends
    /// are the declared kind on a side `declared` names, else the kinds of
    /// `writers`, sorted, each once.
    pub(crate) fn resolved(
        label: &str,
        source_extension: &str,
        declared: Option<EdgeTypeDescriptor>,
        writers: Vec<EdgeWriter>,
    ) -> Self {
        let sources = declared
            .as_ref()
            .and_then(|d| d.source_kind.clone())
            .map(|kind| vec![kind])
            .unwrap_or_else(|| distinct(writers.iter().map(|w| w.kind.as_str())));
        let targets = declared
            .as_ref()
            .and_then(|d| d.target_kind.clone())
            .map(|kind| vec![kind])
            .unwrap_or_else(|| distinct(writers.iter().filter_map(|w| w.target_kind.as_deref())));
        Self {
            label: label.to_string(),
            source_extension: source_extension.to_string(),
            declared,
            writers,
            sources,
            targets,
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// The extension that declares it; an edge type only fields name
    /// belongs to its first writer's extension, in load order.
    pub fn source_extension(&self) -> &str {
        &self.source_extension
    }

    /// Its declaration (description, declared ends, DOT style); `None` for
    /// an edge type only fields name (W021 says so).
    pub fn declared(&self) -> Option<&EdgeTypeDescriptor> {
        self.declared.as_ref()
    }

    /// Every field that writes it: by extension in load order, then kind and
    /// field. A field of a kind's own, its extension's shared fields and
    /// every enhancement's fields alike. A field outside a declared end is
    /// not one (W021).
    pub fn writers(&self) -> &[EdgeWriter] {
        &self.writers
    }

    /// The kinds its edges start at: the declared source kind, else its
    /// writers' kinds; sorted, each once. Empty: nothing declares or writes
    /// one.
    pub fn sources(&self) -> &[String] {
        &self.sources
    }

    /// The kinds its edges end at: the declared target kind, else its
    /// writers' declared target kinds; sorted, each once.
    pub fn targets(&self) -> &[String] {
        &self.targets
    }
}

/// `kinds`, each once, sorted.
fn distinct<'a>(kinds: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut kinds: Vec<String> = kinds.map(str::to_string).collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

/// Every edge type of a registry build, by label.
#[derive(Debug, Default)]
pub struct EdgeRegistry {
    entries: BTreeMap<String, EdgeRegistryEntry>,
    /// (kind, field) -> the label of the edge type that field writes.
    written: HashMap<(String, String), String>,
}

impl EdgeRegistry {
    /// The registry of `entries`, whose writers are the fields `written`
    /// maps. Made only by the registry build.
    pub(crate) fn resolved(
        entries: BTreeMap<String, EdgeRegistryEntry>,
        written: HashMap<(String, String), String>,
    ) -> Self {
        Self { entries, written }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, label: &str) -> Option<&EdgeRegistryEntry> {
        self.entries.get(label)
    }

    pub fn contains(&self, label: &str) -> bool {
        self.entries.contains_key(label)
    }

    /// Every edge type, in label order.
    pub fn iter(&self) -> impl Iterator<Item = &EdgeRegistryEntry> {
        self.entries.values()
    }

    /// The edge type of an edge an entity of `kind` writes through `field`
    /// (a graph edge's label): `None` when that field writes none: no
    /// `edge`, an unregistered field, or a writer outside a declared end.
    pub fn written_by(&self, kind: &str, field: &str) -> Option<&EdgeRegistryEntry> {
        let label = self.written.get(&(kind.to_string(), field.to_string()))?;
        self.entries.get(label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // B:boot_empty_edge_registry — verify unit "edge type set starts with zero entries"
    #[test]
    fn test_edge_registry_new_has_zero_entries() {
        let registry = EdgeRegistry::default();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    // B:boot_empty_edge_registry — verify unit "no edge labels recognized before extension loading"
    #[test]
    fn test_no_edge_labels_recognized_before_extension_loading() {
        let registry = EdgeRegistry::default();
        assert!(registry.get("invariants").is_none());
        assert!(!registry.contains("invariants"));
    }

    // B:boot_empty_edge_registry — verify contract "requires/ensures consistency for empty edge registry boot"
    #[test]
    fn test_boot_empty_edge_registry_contract() {
        // requires: no extensions loaded yet
        let registry = EdgeRegistry::default();
        // ensures: empty
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        // ensures: get returns None
        assert!(registry.get("enforces").is_none());
        // ensures: no field writes an edge type
        assert!(registry.written_by("behavior", "invariants").is_none());
        // ensures: iter yields nothing
        assert_eq!(registry.iter().count(), 0);
    }
}
