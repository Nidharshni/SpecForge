use std::collections::HashMap;

use specforge_graph::FieldForm;
use specforge_protocol_types::{FieldDescriptor, FieldType, ProofRole};

/// A declared field type this host does not read. The registry build
/// reports it as W019 and registers nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownFieldType(pub String);

/// One registered field of one kind: the descriptor its extension declared,
/// the kind it is registered on and the extension that declared the field:
/// the kind's own extension, its shared field's or the enhancement's.
///
/// [`FieldRegistryEntry::new`] is the only way to make one, so an entry's
/// type, enum values and proof role are always its declaration's, read once.
/// There is no second copy to disagree with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRegistryEntry {
    kind_name: String,
    source_extension: String,
    form: FieldForm,
    proof_role: Option<ProofRole>,
    declared: FieldDescriptor,
}

impl FieldRegistryEntry {
    /// `declared`, registered on `kind_name`, declared by `source_extension`.
    ///
    /// The declared type must be a name the host reads (canonical or an
    /// accepted older spelling), else `Err` (W019 at the registry build). The
    /// entry's descriptor names the type canonically (`"boolean"` becomes
    /// `"bool"`), so every reader of `declared()` sees the one name. A
    /// `proof_role` that is not `bound` or `claim` is read as none; the
    /// registry build reports it (W021).
    pub fn new(
        kind_name: &str,
        source_extension: &str,
        mut declared: FieldDescriptor,
    ) -> Result<Self, UnknownFieldType> {
        let field_type = FieldType::parse(&declared.field_type)
            .ok_or_else(|| UnknownFieldType(declared.field_type.clone()))?;
        declared.field_type = field_type.as_str().to_string();
        let proof_role = declared.proof_role.as_deref().and_then(ProofRole::parse);
        let form = FieldForm::new(field_type, declared.enum_values.clone());
        Ok(Self {
            kind_name: kind_name.to_string(),
            source_extension: source_extension.to_string(),
            form,
            proof_role,
            declared,
        })
    }

    /// The kind the field is registered on.
    pub fn kind_name(&self) -> &str {
        &self.kind_name
    }

    /// The field's name, as declared.
    pub fn name(&self) -> &str {
        &self.declared.name
    }

    /// The extension that declared the field.
    pub fn source_extension(&self) -> &str {
        &self.source_extension
    }

    /// How the host reads the field's value.
    pub fn field_type(&self) -> FieldType {
        self.form.field_type()
    }

    /// The values an enum field accepts, as declared; empty for any other
    /// type (and for an enum that declares none).
    pub fn enum_values(&self) -> &[String] {
        self.form.enum_values()
    }

    /// The field's declared type read once (ADR-NEW-01): which stored
    /// values it takes, how a written value is stored as it and the JSON
    /// written for it.
    pub fn form(&self) -> &FieldForm {
        &self.form
    }

    /// The type as messages and hover name it: the type's name, followed by
    /// an enum's declared values: `enum (low, medium, high)`.
    pub fn type_label(&self) -> String {
        match self.enum_values() {
            [] => self.field_type().as_str().to_string(),
            values => format!("{} ({})", self.field_type().as_str(), values.join(", ")),
        }
    }

    /// What the prove pass reads the field as, when it declares a known role.
    pub fn proof_role(&self) -> Option<ProofRole> {
        self.proof_role
    }

    /// The field as its extension declared it (its type named canonically).
    pub fn declared(&self) -> &FieldDescriptor {
        &self.declared
    }
}

#[derive(Debug, Default)]
pub struct FieldRegistry {
    /// Two-level map: kind_name -> field_name -> entry.
    /// Using nested `HashMap<String, _>` instead of `HashMap<(String, String), _>`
    /// so lookups can accept `&str` directly (via `Borrow<str>` on `String`),
    /// avoiding per-call `to_string()` allocations.
    entries: HashMap<String, HashMap<String, FieldRegistryEntry>>,
    /// Total number of registered fields (cached for O(1) len).
    count: usize,
}

impl FieldRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            count: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Look up a field entry by kind and field name.
    /// Zero-allocation: accepts `&str` without converting to `String`.
    pub fn get(&self, kind_name: &str, field_name: &str) -> Option<&FieldRegistryEntry> {
        self.entries.get(kind_name)?.get(field_name)
    }

    /// Check if a field is registered for the given kind.
    /// Zero-allocation: accepts `&str` without converting to `String`.
    pub fn contains(&self, kind_name: &str, field_name: &str) -> bool {
        self.entries
            .get(kind_name)
            .is_some_and(|fields| fields.contains_key(field_name))
    }

    pub fn register(&mut self, entry: FieldRegistryEntry) {
        let kind_map = self
            .entries
            .entry(entry.kind_name().to_string())
            .or_default();
        if !kind_map.contains_key(entry.name()) {
            self.count += 1;
        }
        kind_map.insert(entry.name().to_string(), entry);
    }

    /// The fields of `kind.field`'s target kind that hold the inverse
    /// reference: the field it names `inverse_of`, when its target kind
    /// registers it, and every field of the target kind that names it
    /// `inverse_of`; sorted, each once. Empty for a field with no target
    /// kind, or that is not registered. The one reading of `inverse_of`
    /// ("the field on the target kind that holds the inverse reference",
    /// ADR 0060): trace and the graph's bidirectional pairs share it.
    pub fn inverse_fields(&self, kind: &str, field: &str) -> Vec<&str> {
        let Some(entry) = self.get(kind, field) else {
            return Vec::new();
        };
        let Some(target) = entry.declared().target_kind.as_deref() else {
            return Vec::new();
        };
        let named = entry
            .declared()
            .inverse_of
            .as_deref()
            .and_then(|inverse| self.get(target, inverse))
            .map(FieldRegistryEntry::name);
        let naming_it = self
            .fields_for_kind(target)
            .into_iter()
            .filter(|other| other.declared().inverse_of.as_deref() == Some(field))
            .map(FieldRegistryEntry::name);
        let mut inverse: Vec<&str> = named.into_iter().chain(naming_it).collect();
        inverse.sort_unstable();
        inverse.dedup();
        inverse
    }

    pub fn fields_for_kind(&self, kind_name: &str) -> Vec<&FieldRegistryEntry> {
        self.entries
            .get(kind_name)
            .map(|fields| fields.values().collect())
            .unwrap_or_default()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, &FieldRegistryEntry)> {
        self.entries.iter().flat_map(|(kind, fields)| {
            fields
                .iter()
                .map(move |(field, entry)| (kind.as_str(), field.as_str(), entry))
        })
    }
}
