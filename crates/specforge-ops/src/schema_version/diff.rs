//! What changed between two Graph Protocol schemas, and the version that
//! follows from it.

use std::collections::BTreeMap;
use std::fmt;

use specforge_common::codes;
use specforge_emitter::{
    GraphProtocolSchema, SchemaEdgeType, SchemaEntityKind, SchemaField, SchemaVersion,
};

/// One change between the recorded schema and the generated one. Its
/// [`fmt::Display`] is the one text W053 and every test read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaChange {
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

impl SchemaChange {
    pub fn is_breaking(&self) -> bool {
        matches!(
            self,
            SchemaChange::KindRemoved(_)
                | SchemaChange::EdgeRemoved(_)
                | SchemaChange::FieldRemoved { .. }
                | SchemaChange::FieldAdded { required: true, .. }
                | SchemaChange::FieldTypeChanged { .. }
        )
    }

    pub(super) fn is_addition(&self) -> bool {
        matches!(
            self,
            SchemaChange::KindAdded(_)
                | SchemaChange::EdgeAdded(_)
                | SchemaChange::FieldAdded { .. }
        )
    }
}

impl fmt::Display for SchemaChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaChange::KindAdded(kind) => write!(f, "entity kind `{kind}` was added"),
            SchemaChange::KindRemoved(kind) => write!(f, "entity kind `{kind}` was removed"),
            SchemaChange::EdgeAdded(label) => write!(f, "edge type `{label}` was added"),
            SchemaChange::EdgeRemoved(label) => write!(f, "edge type `{label}` was removed"),
            SchemaChange::FieldAdded {
                kind,
                field,
                required,
            } => {
                let which = if *required { "required" } else { "optional" };
                write!(f, "{which} field `{field}` was added to `{kind}`")
            }
            SchemaChange::FieldRemoved { kind, field } => {
                write!(f, "field `{field}` was removed from `{kind}`")
            }
            SchemaChange::FieldTypeChanged {
                kind,
                field,
                old_type,
                new_type,
            } => write!(
                f,
                "field `{field}` of `{kind}` changed type from `{old_type}` to `{new_type}`"
            ),
            SchemaChange::MetadataChanged {
                kind,
                field: Some(field),
                attribute,
            } => write!(f, "the {attribute} of field `{field}` of `{kind}` changed"),
            SchemaChange::MetadataChanged {
                kind,
                field: None,
                attribute,
            } => write!(f, "the {attribute} of entity kind `{kind}` changed"),
        }
    }
}

/// Every change from `old` to `new`.
pub(super) fn diff(old: &GraphProtocolSchema, new: &GraphProtocolSchema) -> Vec<SchemaChange> {
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
            changes.push(SchemaChange::KindRemoved(name.to_string()));
        }
    }
    for name in new_kinds.keys() {
        if !old_kinds.contains_key(name) {
            changes.push(SchemaChange::KindAdded(name.to_string()));
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
            changes.push(SchemaChange::EdgeRemoved(label.to_string()));
        }
    }
    for label in new_edges.keys() {
        if !old_edges.contains_key(label) {
            changes.push(SchemaChange::EdgeAdded(label.to_string()));
        }
    }

    // Fields per shared kind
    for (name, new_kind) in &new_kinds {
        if let Some(old_kind) = old_kinds.get(name) {
            if old_kind.dot_color != new_kind.dot_color {
                changes.push(SchemaChange::MetadataChanged {
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
                    changes.push(SchemaChange::FieldRemoved {
                        kind: name.to_string(),
                        field: field_name.to_string(),
                    });
                }
            }
            for (field_name, field) in &new_fields {
                if let Some(old_field) = old_fields.get(field_name) {
                    if old_field.field_type != field.field_type {
                        changes.push(SchemaChange::FieldTypeChanged {
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
                            changes.push(SchemaChange::MetadataChanged {
                                kind: name.to_string(),
                                field: Some(field_name.to_string()),
                                attribute: attribute.to_string(),
                            });
                        }
                    }
                } else {
                    changes.push(SchemaChange::FieldAdded {
                        kind: name.to_string(),
                        field: field_name.to_string(),
                        required: field.required,
                    });
                }
            }
        }
    }

    changes.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
    changes
}

/// `previous` moved by what `changes` are: a major version for a breaking
/// change, a minor one for an addition, a patch for anything else; `previous`
/// itself when there is none.
pub(super) fn bumped(previous: &SchemaVersion, changes: &[SchemaChange]) -> SchemaVersion {
    if changes.is_empty() {
        previous.clone()
    } else if changes.iter().any(SchemaChange::is_breaking) {
        SchemaVersion::new(previous.major + 1, 0, 0)
    } else if changes.iter().any(SchemaChange::is_addition) {
        SchemaVersion::new(previous.major, previous.minor + 1, 0)
    } else {
        SchemaVersion::new(previous.major, previous.minor, previous.patch + 1)
    }
}

/// A requested schema version no reader of the current one reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SchemaVersionError {
    pub(crate) requested: SchemaVersion,
    pub(crate) min: SchemaVersion,
    pub(crate) max: SchemaVersion,
    pub(crate) reason: String,
}

impl fmt::Display for SchemaVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", codes::E027, self.reason)
    }
}

/// Whether `requested` is a version of the same major as `max`, not below
/// `min` and not above `max`.
pub(crate) fn negotiate_version(
    requested: &SchemaVersion,
    min: &SchemaVersion,
    max: &SchemaVersion,
) -> Result<(), SchemaVersionError> {
    let refuse = |reason: String| SchemaVersionError {
        requested: requested.clone(),
        min: min.clone(),
        max: max.clone(),
        reason,
    };
    if requested.major != max.major {
        return Err(refuse(format!(
            "requested schema version {requested} has incompatible major version \
             (supported range [{min}, {max}])"
        )));
    }
    if requested < min || requested > max {
        return Err(refuse(format!(
            "requested schema version {requested} is out of range [{min}, {max}]"
        )));
    }
    Ok(())
}
