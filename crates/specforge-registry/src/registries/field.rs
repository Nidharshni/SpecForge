use std::collections::HashMap;

use specforge_protocol_types::{FieldDescriptor, FieldType, ProofRole};

/// A declared field type this host does not read. The registry build
/// reports it as W019 and registers nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownFieldType(pub String);

/// One registered field of one kind: the descriptor its extension declared,
/// the kind it is registered on and the extension it is the kind's through
/// (its own, or the owner an enhancement names).
///
/// [`FieldRegistryEntry::new`] is the only way to make one, so an entry's
/// type, enum values and proof role are always its declaration's, read once.
/// There is no second copy to disagree with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRegistryEntry {
    kind_name: String,
    source_extension: String,
    field_type: FieldType,
    proof_role: Option<ProofRole>,
    declared: FieldDescriptor,
}

impl FieldRegistryEntry {
    /// `declared`, registered on `kind_name` through `source_extension`.
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
        Ok(Self {
            kind_name: kind_name.to_string(),
            source_extension: source_extension.to_string(),
            field_type,
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

    /// The extension the field is the kind's through.
    pub fn source_extension(&self) -> &str {
        &self.source_extension
    }

    /// How the host reads the field's value.
    pub fn field_type(&self) -> FieldType {
        self.field_type
    }

    /// The values an enum field accepts, as declared; empty for any other
    /// type (and for an enum that declares none).
    pub fn enum_values(&self) -> &[String] {
        match self.field_type {
            FieldType::Enum => &self.declared.enum_values,
            _ => &[],
        }
    }

    /// The type as messages and hover name it: the type's name, followed by
    /// an enum's declared values: `enum (low, medium, high)`.
    pub fn type_label(&self) -> String {
        match self.enum_values() {
            [] => self.field_type.as_str().to_string(),
            values => format!("{} ({})", self.field_type.as_str(), values.join(", ")),
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
