use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specforge_common::shape::Shape;
use specforge_diagnostics::{Code, codes};
use specforge_registry::{FieldRegistryEntry, FieldType, RegistryBuild};

// ---------------------------------------------------------------------------
// Slice 1: Schema Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Shape)]
pub struct SchemaVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl SchemaVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            label: None,
        }
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref label) = self.label {
            write!(f, "-{}", label)?;
        }
        Ok(())
    }
}

impl FromStr for SchemaVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (version_part, label) = if let Some(idx) = s.find('-') {
            (&s[..idx], Some(s[idx + 1..].to_string()))
        } else {
            (s, None)
        };

        let parts: Vec<&str> = version_part.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("expected MAJOR.MINOR.PATCH, got '{}'", s));
        }

        let major = parts[0]
            .parse::<u32>()
            .map_err(|e| format!("invalid major: {}", e))?;
        let minor = parts[1]
            .parse::<u32>()
            .map_err(|e| format!("invalid minor: {}", e))?;
        let patch = parts[2]
            .parse::<u32>()
            .map_err(|e| format!("invalid patch: {}", e))?;

        Ok(SchemaVersion {
            major,
            minor,
            patch,
            label,
        })
    }
}

impl Ord for SchemaVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            // C6-03: semver pre-release ordering — `1.0.0-beta` sorts
            // strictly BELOW `1.0.0`. Without this, cmp returned Equal for
            // a pair that Eq distinguishes, breaking Ord/PartialEq
            // consistency for sorted containers.
            .then_with(|| match (&self.label, &other.label) {
                (None, None) => Ordering::Equal,
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (Some(a), Some(b)) => a.cmp(b),
            })
    }
}

impl PartialOrd for SchemaVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Shape)]
pub struct SchemaField {
    pub name: String,
    /// The field's type, named as its extension declares it (ADR 0034). A
    /// schema an older host wrote reads its older names (`"boolean"`).
    pub field_type: FieldType,
    pub required: bool,
    /// An enum field's declared values (present, possibly empty, exactly for
    /// enum fields).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    /// Extension that contributed this field (may differ from the entity's owning extension
    /// when the field comes from an entity enhancement).
    pub source_extension: String,
}

impl SchemaField {
    /// The field's declared type read once: what it takes, how a written
    /// value is stored and the JSON Schema of what is written for it.
    pub fn form(&self) -> specforge_graph::FieldForm {
        specforge_graph::FieldForm::new(
            self.field_type,
            self.enum_values.clone().unwrap_or_default(),
        )
    }

    /// The schema's field for a registered one: everything its declaration
    /// says.
    fn of(entry: &FieldRegistryEntry) -> Self {
        let declared = entry.declared();
        SchemaField {
            name: entry.name().to_string(),
            field_type: entry.field_type(),
            required: declared.required,
            enum_values: (entry.field_type() == FieldType::Enum)
                .then(|| entry.enum_values().to_vec()),
            edge: declared.edge.clone(),
            target_kind: declared.target_kind.clone(),
            description: declared.description.clone(),
            default_value: declared.default_value.clone(),
            source_extension: entry.source_extension().to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Shape)]
pub struct SchemaEntityKind {
    pub name: String,
    pub source_extension: String,
    pub testable: bool,
    /// Registry-declared DOT color for model renderers (C13-03). Absent when
    /// the extension declares none; renderers fall back to palette colors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dot_color: Option<String>,
    pub fields: Vec<SchemaField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Shape)]
pub struct SchemaEdgeType {
    pub label: String,
    pub source_extension: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kinds: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_kinds: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Shape)]
pub struct SchemaExtensionInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Shape)]
#[shape(define)]
pub struct GraphProtocolSchema {
    pub schema_version: SchemaVersion,
    pub extensions: Vec<SchemaExtensionInfo>,
    pub entity_kinds: Vec<SchemaEntityKind>,
    pub edge_types: Vec<SchemaEdgeType>,
}

impl GraphProtocolSchema {
    pub fn empty() -> Self {
        Self {
            schema_version: SchemaVersion::new(1, 0, 0),
            extensions: Vec::new(),
            entity_kinds: Vec::new(),
            edge_types: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaMigrationChange {
    KindAdded(String),
    KindRemoved(String),
    EdgeAdded(String),
    EdgeRemoved(String),
    FieldAdded {
        kind: String,
        field: String,
        required: bool,
    },
    FieldRemoved {
        kind: String,
        field: String,
    },
    FieldTypeChanged {
        kind: String,
        field: String,
        old_type: String,
        new_type: String,
    },
    /// A non-structural attribute changed (a field's description or default,
    /// a kind's DOT color): a patch bump.
    MetadataChanged {
        kind: String,
        field: Option<String>,
        attribute: String,
    },
}

impl SchemaMigrationChange {
    pub fn is_breaking(&self) -> bool {
        matches!(
            self,
            SchemaMigrationChange::KindRemoved(_)
                | SchemaMigrationChange::EdgeRemoved(_)
                | SchemaMigrationChange::FieldRemoved { .. }
                | SchemaMigrationChange::FieldAdded { required: true, .. }
                | SchemaMigrationChange::FieldTypeChanged { .. }
        )
    }
}

impl fmt::Display for SchemaMigrationChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaMigrationChange::KindAdded(kind) => write!(f, "entity kind `{kind}` was added"),
            SchemaMigrationChange::KindRemoved(kind) => {
                write!(f, "entity kind `{kind}` was removed")
            }
            SchemaMigrationChange::EdgeAdded(label) => write!(f, "edge type `{label}` was added"),
            SchemaMigrationChange::EdgeRemoved(label) => {
                write!(f, "edge type `{label}` was removed")
            }
            SchemaMigrationChange::FieldAdded {
                kind,
                field,
                required,
            } => {
                let which = if *required { "required" } else { "optional" };
                write!(f, "{which} field `{field}` was added to `{kind}`")
            }
            SchemaMigrationChange::FieldRemoved { kind, field } => {
                write!(f, "field `{field}` was removed from `{kind}`")
            }
            SchemaMigrationChange::FieldTypeChanged {
                kind,
                field,
                old_type,
                new_type,
            } => write!(
                f,
                "field `{field}` of `{kind}` changed type from `{old_type}` to `{new_type}`"
            ),
            SchemaMigrationChange::MetadataChanged {
                kind,
                field: Some(field),
                attribute,
            } => write!(f, "the {attribute} of field `{field}` of `{kind}` changed"),
            SchemaMigrationChange::MetadataChanged {
                kind,
                field: None,
                attribute,
            } => write!(f, "the {attribute} of entity kind `{kind}` changed"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaMigration {
    pub changes: Vec<SchemaMigrationChange>,
}

impl SchemaMigration {
    pub fn has_breaking_changes(&self) -> bool {
        self.changes.iter().any(|c| c.is_breaking())
    }

    pub fn has_additions(&self) -> bool {
        self.changes.iter().any(|c| {
            matches!(
                c,
                SchemaMigrationChange::KindAdded(_)
                    | SchemaMigrationChange::EdgeAdded(_)
                    | SchemaMigrationChange::FieldAdded { .. }
            )
        })
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaCompatibility {
    pub requested: SchemaVersion,
    pub resolved: SchemaVersion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaVersionError {
    pub requested: SchemaVersion,
    pub min: SchemaVersion,
    pub max: SchemaVersion,
    pub reason: String,
}

impl SchemaVersionError {
    /// The diagnostic this failure is: E027.
    pub fn code(&self) -> Code {
        codes::E027
    }
}

impl fmt::Display for SchemaVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", codes::E027, self.reason)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaCacheEntry {
    pub schema: GraphProtocolSchema,
    pub content_hash: String,
}

/// Pointer to a published Graph Protocol schema (C6-07): the same `$id` URL
/// the publisher emits plus the content hash from the schema cache, letting
/// scoped exports name their schema without embedding the whole document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Shape)]
#[shape(define)]
pub struct SchemaRefBlock {
    pub url: String,
    pub content_hash: String,
}

impl SchemaRefBlock {
    pub fn for_schema(schema: &GraphProtocolSchema) -> Self {
        Self {
            url: schema_url(schema, None),
            content_hash: content_hash(schema),
        }
    }
}

/// The identity of the published schema of `schema`: the graph document's,
/// or, for `variant` (`context`, `brief`), that format's own.
pub(crate) fn schema_url(schema: &GraphProtocolSchema, variant: Option<&str>) -> String {
    let graph = format!(
        "https://specforge.dev/schema/graph-protocol-v{}.json",
        schema.schema_version
    );
    match variant {
        None => graph,
        Some(variant) => graph.replace(".json", &format!("-{variant}.json")),
    }
}

/// How the schema travels with a V2 export: embedded document (full exports)
/// or reference (scoped exports, C6-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchemaAttachment {
    Embedded,
    Referenced,
}

// ---------------------------------------------------------------------------
// Slice 2: Generate Schema from Registries
// ---------------------------------------------------------------------------

/// The Graph Protocol schema of `build`: its kinds with their fields, its
/// edge types with the ends the build resolved, and its extensions. The
/// schema is generated from the registry build alone (ADR 0060); nothing is
/// inferred here.
pub fn generate_schema(build: &RegistryBuild) -> GraphProtocolSchema {
    let (kinds, edges, fields) = (build.kinds(), build.edges(), build.fields());
    let ext_infos: Vec<SchemaExtensionInfo> = build
        .extension_info()
        .map(|(name, version)| SchemaExtensionInfo {
            name: name.to_string(),
            version: version.to_string(),
        })
        .collect();

    let mut entity_kinds: Vec<SchemaEntityKind> = kinds
        .iter()
        .map(|(_, entry)| {
            let mut kind_fields: Vec<SchemaField> = fields
                .fields_for_kind(&entry.kind_name)
                .into_iter()
                .map(SchemaField::of)
                .collect();
            kind_fields.sort_by(|a, b| a.name.cmp(&b.name));

            SchemaEntityKind {
                name: entry.kind_name.clone(),
                source_extension: entry.source_extension.clone(),
                testable: entry.testable,
                dot_color: entry.declared.dot_color.clone(),
                fields: kind_fields,
            }
        })
        .collect();
    entity_kinds.sort_by(|a, b| a.name.cmp(&b.name));

    // The ends the registry build resolved (ADR 0060): the declared kind on
    // a side it declares, else the kinds of the fields that write the edge
    // type; a side nothing resolves is absent.
    let mut edge_types: Vec<SchemaEdgeType> = edges
        .iter()
        .map(|entry| SchemaEdgeType {
            label: entry.label().to_string(),
            source_extension: entry.source_extension().to_string(),
            source_kinds: (!entry.sources().is_empty()).then(|| entry.sources().to_vec()),
            target_kinds: (!entry.targets().is_empty()).then(|| entry.targets().to_vec()),
        })
        .collect();

    edge_types.sort_by(|a, b| a.label.cmp(&b.label));

    GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
        extensions: ext_infos,
        entity_kinds,
        edge_types,
    }
}

// ---------------------------------------------------------------------------
// Slice 4: Schema Diffing
// ---------------------------------------------------------------------------

pub fn diff_schemas(old: &GraphProtocolSchema, new: &GraphProtocolSchema) -> SchemaMigration {
    let mut changes = Vec::new();

    // Entity kinds
    let old_kinds: BTreeMap<&str, &SchemaEntityKind> = old
        .entity_kinds
        .iter()
        .map(|k| (k.name.as_str(), k))
        .collect();
    let new_kinds: BTreeMap<&str, &SchemaEntityKind> = new
        .entity_kinds
        .iter()
        .map(|k| (k.name.as_str(), k))
        .collect();

    for name in old_kinds.keys() {
        if !new_kinds.contains_key(name) {
            changes.push(SchemaMigrationChange::KindRemoved(name.to_string()));
        }
    }
    for name in new_kinds.keys() {
        if !old_kinds.contains_key(name) {
            changes.push(SchemaMigrationChange::KindAdded(name.to_string()));
        }
    }

    // Edge types
    let old_edges: BTreeMap<&str, &SchemaEdgeType> = old
        .edge_types
        .iter()
        .map(|e| (e.label.as_str(), e))
        .collect();
    let new_edges: BTreeMap<&str, &SchemaEdgeType> = new
        .edge_types
        .iter()
        .map(|e| (e.label.as_str(), e))
        .collect();

    for label in old_edges.keys() {
        if !new_edges.contains_key(label) {
            changes.push(SchemaMigrationChange::EdgeRemoved(label.to_string()));
        }
    }
    for label in new_edges.keys() {
        if !old_edges.contains_key(label) {
            changes.push(SchemaMigrationChange::EdgeAdded(label.to_string()));
        }
    }

    // Fields per shared kind
    for (name, new_kind) in &new_kinds {
        if let Some(old_kind) = old_kinds.get(name) {
            if old_kind.dot_color != new_kind.dot_color {
                changes.push(SchemaMigrationChange::MetadataChanged {
                    kind: name.to_string(),
                    field: None,
                    attribute: "dot_color".to_string(),
                });
            }
            let old_fields: BTreeMap<&str, &SchemaField> = old_kind
                .fields
                .iter()
                .map(|f| (f.name.as_str(), f))
                .collect();
            let new_fields: BTreeMap<&str, &SchemaField> = new_kind
                .fields
                .iter()
                .map(|f| (f.name.as_str(), f))
                .collect();

            for field_name in old_fields.keys() {
                if !new_fields.contains_key(field_name) {
                    changes.push(SchemaMigrationChange::FieldRemoved {
                        kind: name.to_string(),
                        field: field_name.to_string(),
                    });
                }
            }
            for (field_name, field) in &new_fields {
                if let Some(old_field) = old_fields.get(field_name) {
                    if old_field.field_type != field.field_type {
                        changes.push(SchemaMigrationChange::FieldTypeChanged {
                            kind: name.to_string(),
                            field: field_name.to_string(),
                            old_type: old_field.field_type.as_str().to_string(),
                            new_type: field.field_type.as_str().to_string(),
                        });
                    }
                    for (attribute, changed) in [
                        ("description", old_field.description != field.description),
                        (
                            "default_value",
                            old_field.default_value != field.default_value,
                        ),
                    ] {
                        if changed {
                            changes.push(SchemaMigrationChange::MetadataChanged {
                                kind: name.to_string(),
                                field: Some(field_name.to_string()),
                                attribute: attribute.to_string(),
                            });
                        }
                    }
                } else {
                    changes.push(SchemaMigrationChange::FieldAdded {
                        kind: name.to_string(),
                        field: field_name.to_string(),
                        required: field.required,
                    });
                }
            }
        }
    }

    changes.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
    SchemaMigration { changes }
}

pub fn diff_schemas_optional(
    old: Option<&GraphProtocolSchema>,
    new: &GraphProtocolSchema,
) -> SchemaMigration {
    match old {
        Some(old_schema) => diff_schemas(old_schema, new),
        None => {
            let mut changes = Vec::new();
            for kind in &new.entity_kinds {
                changes.push(SchemaMigrationChange::KindAdded(kind.name.clone()));
            }
            for edge in &new.edge_types {
                changes.push(SchemaMigrationChange::EdgeAdded(edge.label.clone()));
            }
            SchemaMigration { changes }
        }
    }
}

// ---------------------------------------------------------------------------
// Slice 5: Compute Schema Version
// ---------------------------------------------------------------------------

pub fn compute_schema_version(
    migration: &SchemaMigration,
    previous: Option<&SchemaVersion>,
) -> SchemaVersion {
    let prev = match previous {
        Some(v) => v.clone(),
        None => return SchemaVersion::new(1, 0, 0),
    };

    if migration.is_empty() {
        return prev;
    }

    if migration.has_breaking_changes() {
        SchemaVersion::new(prev.major + 1, 0, 0)
    } else if migration.has_additions() {
        SchemaVersion::new(prev.major, prev.minor + 1, 0)
    } else {
        SchemaVersion::new(prev.major, prev.minor, prev.patch + 1)
    }
}

// ---------------------------------------------------------------------------
// Slice 6: Version Negotiation
// ---------------------------------------------------------------------------

#[allow(clippy::result_large_err)]
pub fn negotiate_version(
    requested: &SchemaVersion,
    min: &SchemaVersion,
    max: &SchemaVersion,
) -> Result<SchemaCompatibility, SchemaVersionError> {
    if requested.major != max.major {
        return Err(SchemaVersionError {
            requested: requested.clone(),
            min: min.clone(),
            max: max.clone(),
            reason: format!(
                "requested schema version {} has incompatible major version \
                 (supported range [{}, {}])",
                requested, min, max
            ),
        });
    }

    if requested < min || requested > max {
        return Err(SchemaVersionError {
            requested: requested.clone(),
            min: min.clone(),
            max: max.clone(),
            reason: format!(
                "requested schema version {} is out of range [{}, {}]",
                requested, min, max
            ),
        });
    }

    Ok(SchemaCompatibility {
        requested: requested.clone(),
        resolved: requested.clone(),
    })
}

// ---------------------------------------------------------------------------
// Slice 7: Schema Cache Persistence
// ---------------------------------------------------------------------------

/// The content hash a schema cache entry and a scoped export's
/// `schema_ref` carry: SHA-256 of the schema's JSON.
pub fn content_hash(schema: &GraphProtocolSchema) -> String {
    let json = serde_json::to_string(schema).expect("schema serialization cannot fail");
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// Slice 6b: Version Negotiation — default to latest
// ---------------------------------------------------------------------------
