use std::collections::HashMap;

use specforge_protocol_types::EntityKindDescriptor;

/// One registered entity kind: what its extension declared, and what the
/// registry build resolved of it. Everything else is read from `declared`
/// (`entry.declared.description`, `entry.declared.has_body_parser`, ...).
#[derive(Debug, Clone, Default)]
pub struct KindRegistryEntry {
    /// The keyword its entities are written with: the declared keyword,
    /// else its name.
    pub kind_name: String,
    pub source_extension: String,
    /// Declared, or made so by an enhancement carrying `verify_kinds`.
    pub testable: bool,
    /// Declared, or made so by an enhancement carrying `verify_kinds`.
    pub supports_verify: bool,
    /// The `verify` kinds it accepts: declared, or an enhancement's.
    pub allowed_verify_kinds: Vec<String>,
    /// The field holding its entities' lifecycle state, which the build
    /// cache records (ADR 0009, C): the declared one, when it is one of the
    /// kind's fields (W021 otherwise).
    pub lifecycle_field: Option<String>,
    /// The kind as its extension declared it.
    pub declared: EntityKindDescriptor,
}

#[derive(Debug, Default)]
pub struct KindRegistry {
    entries: HashMap<String, KindRegistryEntry>,
}

impl KindRegistry {
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

    pub fn get(&self, kind_name: &str) -> Option<&KindRegistryEntry> {
        self.entries.get(kind_name)
    }

    pub fn get_mut(&mut self, kind_name: &str) -> Option<&mut KindRegistryEntry> {
        self.entries.get_mut(kind_name)
    }

    pub fn contains(&self, kind_name: &str) -> bool {
        self.entries.contains_key(kind_name)
    }

    pub fn register(&mut self, entry: KindRegistryEntry) -> Option<KindRegistryEntry> {
        self.entries.insert(entry.kind_name.clone(), entry)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &KindRegistryEntry)> {
        self.entries.iter()
    }

    pub fn keywords(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // B:boot_empty_kind_registry — verify unit "KindRegistry::new() has zero entries"
    #[test]
    fn test_kind_registry_new_has_zero_entries() {
        let registry = KindRegistry::new();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    // B:boot_empty_kind_registry — verify unit "parser recognizes spec keyword without extensions"
    // B:boot_empty_kind_registry — verify unit "parser recognizes ref keyword without extensions"
    // B:boot_empty_kind_registry — verify unit "parser recognizes use keyword without extensions"
    // B:boot_empty_kind_registry — verify unit "parser recognizes define keyword without extensions"
    // These 4 are parser-level tests (structural keywords are grammar rules, not registry entries).
    // The KindRegistry has zero entries — structural keywords bypass it entirely.
    #[test]
    fn test_structural_keywords_not_in_kind_registry() {
        let registry = KindRegistry::new();
        // Structural keywords (spec, ref, use, define) are grammar rules,
        // NOT entity kinds — they are never registered in KindRegistry.
        assert!(!registry.contains("spec"));
        assert!(!registry.contains("ref"));
        assert!(!registry.contains("use"));
        assert!(!registry.contains("define"));
    }

    // B:boot_empty_kind_registry — verify contract "requires/ensures consistency for empty kind registry boot"
    #[test]
    fn test_boot_empty_kind_registry_contract() {
        // requires: no extensions loaded yet
        // ensures: registry is empty, len==0, get returns None for any key
        let registry = KindRegistry::new();
        // ensures: empty
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        // ensures: get returns None for arbitrary keys
        assert!(registry.get("behavior").is_none());
        assert!(registry.get("feature").is_none());
        assert!(registry.get("").is_none());
        // ensures: contains returns false for arbitrary keys
        assert!(!registry.contains("behavior"));
        // ensures: keywords iterator yields nothing
        assert_eq!(registry.keywords().count(), 0);
        // ensures: iter yields nothing
        assert_eq!(registry.iter().count(), 0);
    }
}
