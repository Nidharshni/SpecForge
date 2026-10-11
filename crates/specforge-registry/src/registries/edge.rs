use std::collections::HashMap;

use specforge_protocol_types::EdgeTypeDescriptor;

/// One registered edge type, as its extension declared it. An edge label
/// fields map to without any extension declaring it is registered with
/// its label alone and every field that writes it ([`EdgeWriter`]), in
/// load order: its ends are those writers'.
#[derive(Debug, Clone, Default)]
pub struct EdgeRegistryEntry {
    /// The declaring extension; for an undeclared edge, its first writer's.
    pub source_extension: String,
    pub declared: EdgeTypeDescriptor,
    /// Every field that writes the edge type without declaring it, in load
    /// order; empty for a declared edge type.
    pub writers: Vec<EdgeWriter>,
}

/// A field that writes an edge type no extension declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeWriter {
    pub extension: String,
    /// The keyword of the kind the field is on.
    pub kind: String,
    pub field: String,
    /// The kind the field declares it targets.
    pub target_kind: Option<String>,
}

impl EdgeRegistryEntry {
    /// The kinds at its source end: the declared source kind, else every
    /// writer's kind, each once, in load order. Empty: any kind.
    pub fn source_kinds(&self) -> Vec<&str> {
        match &self.declared.source_kind {
            Some(kind) => vec![kind.as_str()],
            None => distinct(self.writers.iter().map(|w| w.kind.as_str())),
        }
    }

    /// The kinds at its target end: the declared target kind, else every
    /// writer's target kind, each once, in load order. Empty: any kind.
    pub fn target_kinds(&self) -> Vec<&str> {
        match &self.declared.target_kind {
            Some(kind) => vec![kind.as_str()],
            None => distinct(self.writers.iter().filter_map(|w| w.target_kind.as_deref())),
        }
    }
}

/// `kinds`, each once, in their first order.
fn distinct<'a>(kinds: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut seen = Vec::new();
    for kind in kinds {
        if !seen.contains(&kind) {
            seen.push(kind);
        }
    }
    seen
}

#[derive(Debug, Default)]
pub struct EdgeRegistry {
    entries: HashMap<String, EdgeRegistryEntry>,
}

impl EdgeRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
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

    pub fn register(&mut self, entry: EdgeRegistryEntry) -> Option<EdgeRegistryEntry> {
        self.entries.insert(entry.declared.label.clone(), entry)
    }

    /// `writer` writes the edge type `label`, which no extension declares:
    /// appended to its writers, the entry registered with its first.
    pub(crate) fn record_writer(&mut self, label: &str, writer: EdgeWriter) {
        let entry = self
            .entries
            .entry(label.to_string())
            .or_insert_with(|| EdgeRegistryEntry {
                source_extension: writer.extension.clone(),
                declared: EdgeTypeDescriptor {
                    label: label.to_string(),
                    ..Default::default()
                },
                writers: Vec::new(),
            });
        entry.writers.push(writer);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &EdgeRegistryEntry)> {
        self.entries.iter()
    }

    pub fn labels(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // B:boot_empty_edge_registry — verify unit "edge type set starts with zero entries"
    #[test]
    fn test_edge_registry_new_has_zero_entries() {
        let registry = EdgeRegistry::new();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    // B:boot_empty_edge_registry — verify unit "no edge labels recognized before extension loading"
    #[test]
    fn test_no_edge_labels_recognized_before_extension_loading() {
        let registry = EdgeRegistry::new();
        assert!(registry.get("invariants").is_none());
        assert!(!registry.contains("invariants"));
    }

    // B:boot_empty_edge_registry — verify contract "requires/ensures consistency for empty edge registry boot"
    #[test]
    fn test_boot_empty_edge_registry_contract() {
        // requires: no extensions loaded yet
        let registry = EdgeRegistry::new();
        // ensures: empty
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        // ensures: get returns None
        assert!(registry.get("enforces").is_none());
        // ensures: labels yields nothing
        assert_eq!(registry.labels().count(), 0);
        // ensures: iter yields nothing
        assert_eq!(registry.iter().count(), 0);
    }
}
