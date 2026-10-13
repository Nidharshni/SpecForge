//! What changed between two Graph Protocol schemas, and the version that
//! follows from it (ADR 0067 D2).
//!
//! The rule: within a major version a reader of an earlier version reads
//! every later document. A set the schema publishes (kinds, edge types, a
//! kind's fields, an enum's values, an edge type's ends) grows in a minor
//! version and shrinks only in a major one; a single-valued attribute that
//! shapes a document (a field's type, whether it is required, the edge type
//! it writes, the kind it targets) changes only in a major version; and
//! documentation and provenance (descriptions, defaults, colours, the
//! testable flag, which extension contributed what, extension versions,
//! load order) move the patch number. A field added as required is the one
//! growth that is major: an earlier document lacks it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use specforge_emitter::{
    GraphProtocolSchema, SchemaEdgeType, SchemaEntityKind, SchemaExtensionInfo, SchemaField,
    SchemaVersion,
};

/// One change between the recorded schema and the generated one. Its
/// [`fmt::Display`] is the one text W053 and every test read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaChange {
    ExtensionAdded(String),
    ExtensionRemoved(String),
    ExtensionVersion {
        name: String,
        before: String,
        after: String,
    },
    /// The extensions' load order changed (the same names, another order).
    LoadOrder,
    KindAdded(String),
    KindRemoved(String),
    Kind {
        kind: String,
        change: AttributeChange,
    },
    FieldAdded {
        kind: String,
        field: String,
        required: bool,
    },
    FieldRemoved {
        kind: String,
        field: String,
    },
    Field {
        kind: String,
        field: String,
        change: AttributeChange,
    },
    EdgeTypeAdded(String),
    EdgeTypeRemoved(String),
    EdgeType {
        label: String,
        change: AttributeChange,
    },
}

/// How one attribute of a kind, a field or an edge type changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeChange {
    /// A single-valued attribute; `None` is "not set". A flag is `true` or
    /// `false`.
    Changed {
        attribute: Scalar,
        before: Option<String>,
        after: Option<String>,
    },
    /// A set-valued attribute gained these members (in the generated order).
    Gained {
        attribute: Members,
        members: Vec<String>,
    },
    /// ... lost these members (in the recorded order).
    Lost {
        attribute: Members,
        members: Vec<String>,
    },
    /// The same members in another order.
    Reordered { attribute: Members },
}

/// The single-valued attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scalar {
    SourceExtension,
    Testable,
    DotColor,
    FieldType,
    Required,
    Edge,
    TargetKind,
    Description,
    DefaultValue,
}

/// The set-valued attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Members {
    EnumValues,
    SourceKinds,
    TargetKinds,
}

/// How far a change moves the version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

impl SchemaChange {
    /// The classification (ADR 0067's table): one exhaustive match, no `_`
    /// arm.
    pub fn bump(&self) -> Bump {
        match self {
            Self::ExtensionAdded(_)
            | Self::ExtensionRemoved(_)
            | Self::ExtensionVersion { .. }
            | Self::LoadOrder => Bump::Patch,
            Self::KindAdded(_) | Self::EdgeTypeAdded(_) => Bump::Minor,
            Self::FieldAdded {
                required: false, ..
            } => Bump::Minor,
            Self::FieldAdded { required: true, .. }
            | Self::KindRemoved(_)
            | Self::FieldRemoved { .. }
            | Self::EdgeTypeRemoved(_) => Bump::Major,
            Self::Kind { change, .. }
            | Self::Field { change, .. }
            | Self::EdgeType { change, .. } => change.bump(),
        }
    }
}

impl AttributeChange {
    fn bump(&self) -> Bump {
        match self {
            Self::Changed { attribute, .. } => match attribute {
                Scalar::FieldType | Scalar::Required | Scalar::Edge | Scalar::TargetKind => {
                    Bump::Major
                }
                Scalar::SourceExtension
                | Scalar::Testable
                | Scalar::DotColor
                | Scalar::Description
                | Scalar::DefaultValue => Bump::Patch,
            },
            Self::Gained { .. } => Bump::Minor,
            Self::Lost { .. } => Bump::Major,
            Self::Reordered { .. } => Bump::Patch,
        }
    }
}

/// What an attribute belongs to, for the text of a change.
enum Subject<'a> {
    Kind(&'a str),
    Field { kind: &'a str, field: &'a str },
    EdgeType(&'a str),
}

impl fmt::Display for Subject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Subject::Kind(kind) => write!(f, "entity kind `{kind}`"),
            Subject::Field { kind, field } => write!(f, "field `{field}` of `{kind}`"),
            Subject::EdgeType(label) => write!(f, "edge type `{label}`"),
        }
    }
}

impl Scalar {
    fn name(self) -> &'static str {
        match self {
            Scalar::SourceExtension => "source extension",
            Scalar::Testable => "testable flag",
            Scalar::DotColor => "DOT color",
            Scalar::FieldType => "type",
            Scalar::Required => "required flag",
            Scalar::Edge => "edge type",
            Scalar::TargetKind => "target kind",
            Scalar::Description => "description",
            Scalar::DefaultValue => "default value",
        }
    }

    /// A flag shows bare (`true`), everything else in backticks.
    fn is_flag(self) -> bool {
        matches!(self, Scalar::Testable | Scalar::Required)
    }
}

impl Members {
    fn name(self) -> &'static str {
        match self {
            Members::EnumValues => "enum values",
            Members::SourceKinds => "source kinds",
            Members::TargetKinds => "target kinds",
        }
    }
}

impl AttributeChange {
    fn describe(&self, subject: &Subject, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let quoted = |members: &[String]| {
            members
                .iter()
                .map(|member| format!("`{member}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self {
            // Descriptions are prose: the text says it changed, not how.
            Self::Changed {
                attribute: Scalar::Description,
                ..
            } => write!(f, "the description of {subject} changed"),
            Self::Changed {
                attribute,
                before,
                after,
            } => {
                let show = |value: &Option<String>| match value {
                    None => "none".to_string(),
                    Some(value) if attribute.is_flag() => value.clone(),
                    Some(value) => format!("`{value}`"),
                };
                write!(
                    f,
                    "the {} of {subject} changed from {} to {}",
                    attribute.name(),
                    show(before),
                    show(after)
                )
            }
            Self::Gained { attribute, members } => {
                write!(
                    f,
                    "the {} of {subject} gained {}",
                    attribute.name(),
                    quoted(members)
                )
            }
            Self::Lost { attribute, members } => {
                write!(
                    f,
                    "the {} of {subject} lost {}",
                    attribute.name(),
                    quoted(members)
                )
            }
            Self::Reordered { attribute } => {
                write!(f, "the {} of {subject} were reordered", attribute.name())
            }
        }
    }
}

impl fmt::Display for SchemaChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExtensionAdded(name) => write!(f, "extension `{name}` was added"),
            Self::ExtensionRemoved(name) => write!(f, "extension `{name}` was removed"),
            Self::ExtensionVersion {
                name,
                before,
                after,
            } => write!(
                f,
                "extension `{name}` went from version `{before}` to `{after}`"
            ),
            Self::LoadOrder => write!(f, "the load order of the extensions changed"),
            Self::KindAdded(kind) => write!(f, "entity kind `{kind}` was added"),
            Self::KindRemoved(kind) => write!(f, "entity kind `{kind}` was removed"),
            Self::Kind { kind, change } => change.describe(&Subject::Kind(kind), f),
            Self::FieldAdded {
                kind,
                field,
                required,
            } => {
                let which = if *required { "required" } else { "optional" };
                write!(f, "{which} field `{field}` was added to `{kind}`")
            }
            Self::FieldRemoved { kind, field } => {
                write!(f, "field `{field}` was removed from `{kind}`")
            }
            Self::Field {
                kind,
                field,
                change,
            } => change.describe(&Subject::Field { kind, field }, f),
            Self::EdgeTypeAdded(label) => write!(f, "edge type `{label}` was added"),
            Self::EdgeTypeRemoved(label) => write!(f, "edge type `{label}` was removed"),
            Self::EdgeType { label, change } => change.describe(&Subject::EdgeType(label), f),
        }
    }
}

/// `before` and `after` as a change of `attribute`, when they differ.
fn scalar(
    attribute: Scalar,
    before: Option<String>,
    after: Option<String>,
) -> Option<AttributeChange> {
    (before != after).then_some(AttributeChange::Changed {
        attribute,
        before,
        after,
    })
}

/// The changes of a set-valued attribute: members lost (in the recorded
/// order), then members gained (in the generated order), then, when neither,
/// a reordering. An absent set is the empty set (ADR 0060: a side nothing
/// resolves is absent).
fn members(attribute: Members, old: &[String], new: &[String]) -> Vec<AttributeChange> {
    let lost: Vec<String> = old.iter().filter(|m| !new.contains(m)).cloned().collect();
    let gained: Vec<String> = new.iter().filter(|m| !old.contains(m)).cloned().collect();
    let mut changes = Vec::new();
    if !lost.is_empty() {
        changes.push(AttributeChange::Lost {
            attribute,
            members: lost,
        });
    }
    if !gained.is_empty() {
        changes.push(AttributeChange::Gained {
            attribute,
            members: gained,
        });
    }
    if changes.is_empty() && old != new {
        changes.push(AttributeChange::Reordered { attribute });
    }
    changes
}

fn flag(value: bool) -> Option<String> {
    Some(value.to_string())
}

/// Every change from `old` to `new`, in schema order: extensions (added,
/// removed, version, by name), their load order, then the kinds by name (a
/// kind's addition or removal, or its own attributes in declaration order and
/// then its fields by name), then the edge types by label. The version fields
/// are not compared (the version is the output). Each `Schema*` value is
/// destructured whole, without `..`: an attribute added to a type does not
/// compile until it is compared and classified.
pub(super) fn diff(old: &GraphProtocolSchema, new: &GraphProtocolSchema) -> Vec<SchemaChange> {
    let GraphProtocolSchema {
        schema_version: _,
        extensions: old_extensions,
        entity_kinds: old_kinds,
        edge_types: old_edges,
    } = old;
    let GraphProtocolSchema {
        schema_version: _,
        extensions: new_extensions,
        entity_kinds: new_kinds,
        edge_types: new_edges,
    } = new;

    let mut changes = Vec::new();
    diff_extensions(old_extensions, new_extensions, &mut changes);

    let old_kinds: BTreeMap<&str, &SchemaEntityKind> =
        old_kinds.iter().map(|k| (k.name.as_str(), k)).collect();
    let new_kinds: BTreeMap<&str, &SchemaEntityKind> =
        new_kinds.iter().map(|k| (k.name.as_str(), k)).collect();
    for name in names(&old_kinds, &new_kinds) {
        match (old_kinds.get(name), new_kinds.get(name)) {
            (None, Some(_)) => changes.push(SchemaChange::KindAdded(name.to_string())),
            (Some(_), None) => changes.push(SchemaChange::KindRemoved(name.to_string())),
            (Some(old), Some(new)) => diff_kind(name, old, new, &mut changes),
            (None, None) => unreachable!("a name comes from one of the two maps"),
        }
    }

    let old_edges: BTreeMap<&str, &SchemaEdgeType> =
        old_edges.iter().map(|e| (e.label.as_str(), e)).collect();
    let new_edges: BTreeMap<&str, &SchemaEdgeType> =
        new_edges.iter().map(|e| (e.label.as_str(), e)).collect();
    for label in names(&old_edges, &new_edges) {
        match (old_edges.get(label), new_edges.get(label)) {
            (None, Some(_)) => changes.push(SchemaChange::EdgeTypeAdded(label.to_string())),
            (Some(_), None) => changes.push(SchemaChange::EdgeTypeRemoved(label.to_string())),
            (Some(old), Some(new)) => diff_edge_type(label, old, new, &mut changes),
            (None, None) => unreachable!("a name comes from one of the two maps"),
        }
    }
    changes
}

/// The names in either map, sorted.
fn names<'a, V>(old: &BTreeMap<&'a str, V>, new: &BTreeMap<&'a str, V>) -> BTreeSet<&'a str> {
    old.keys().chain(new.keys()).copied().collect()
}

fn diff_extensions(
    old: &[SchemaExtensionInfo],
    new: &[SchemaExtensionInfo],
    changes: &mut Vec<SchemaChange>,
) {
    let versions = |extensions: &[SchemaExtensionInfo]| -> BTreeMap<String, String> {
        extensions
            .iter()
            .map(|extension| {
                let SchemaExtensionInfo { name, version } = extension;
                (name.clone(), version.clone())
            })
            .collect()
    };
    let (old_versions, new_versions) = (versions(old), versions(new));
    for name in new_versions
        .keys()
        .filter(|n| !old_versions.contains_key(*n))
    {
        changes.push(SchemaChange::ExtensionAdded(name.clone()));
    }
    for name in old_versions
        .keys()
        .filter(|n| !new_versions.contains_key(*n))
    {
        changes.push(SchemaChange::ExtensionRemoved(name.clone()));
    }
    for (name, after) in &new_versions {
        if let Some(before) = old_versions.get(name)
            && before != after
        {
            changes.push(SchemaChange::ExtensionVersion {
                name: name.clone(),
                before: before.clone(),
                after: after.clone(),
            });
        }
    }
    let shared_order = |from: &[SchemaExtensionInfo], other: &BTreeMap<String, String>| {
        from.iter()
            .filter(|e| other.contains_key(&e.name))
            .map(|e| e.name.clone())
            .collect::<Vec<_>>()
    };
    if shared_order(old, &new_versions) != shared_order(new, &old_versions) {
        changes.push(SchemaChange::LoadOrder);
    }
}

fn diff_kind(
    name: &str,
    old: &SchemaEntityKind,
    new: &SchemaEntityKind,
    changes: &mut Vec<SchemaChange>,
) {
    let SchemaEntityKind {
        name: _,
        source_extension: old_source,
        testable: old_testable,
        dot_color: old_dot_color,
        fields: old_fields,
    } = old;
    let SchemaEntityKind {
        name: _,
        source_extension: new_source,
        testable: new_testable,
        dot_color: new_dot_color,
        fields: new_fields,
    } = new;

    let attributes = [
        scalar(
            Scalar::SourceExtension,
            Some(old_source.clone()),
            Some(new_source.clone()),
        ),
        scalar(Scalar::Testable, flag(*old_testable), flag(*new_testable)),
        scalar(
            Scalar::DotColor,
            old_dot_color.clone(),
            new_dot_color.clone(),
        ),
    ];
    for change in attributes.into_iter().flatten() {
        changes.push(SchemaChange::Kind {
            kind: name.to_string(),
            change,
        });
    }

    let old_fields: BTreeMap<&str, &SchemaField> =
        old_fields.iter().map(|f| (f.name.as_str(), f)).collect();
    let new_fields: BTreeMap<&str, &SchemaField> =
        new_fields.iter().map(|f| (f.name.as_str(), f)).collect();
    for field in names(&old_fields, &new_fields) {
        match (old_fields.get(field), new_fields.get(field)) {
            (None, Some(added)) => changes.push(SchemaChange::FieldAdded {
                kind: name.to_string(),
                field: field.to_string(),
                required: added.required,
            }),
            (Some(_), None) => changes.push(SchemaChange::FieldRemoved {
                kind: name.to_string(),
                field: field.to_string(),
            }),
            (Some(old), Some(new)) => diff_field(name, field, old, new, changes),
            (None, None) => unreachable!("a name comes from one of the two maps"),
        }
    }
}

fn diff_field(
    kind: &str,
    field: &str,
    old: &SchemaField,
    new: &SchemaField,
    changes: &mut Vec<SchemaChange>,
) {
    let SchemaField {
        name: _,
        field_type: old_type,
        required: old_required,
        enum_values: old_enum_values,
        edge: old_edge,
        target_kind: old_target_kind,
        description: old_description,
        default_value: old_default_value,
        source_extension: old_source,
    } = old;
    let SchemaField {
        name: _,
        field_type: new_type,
        required: new_required,
        enum_values: new_enum_values,
        edge: new_edge,
        target_kind: new_target_kind,
        description: new_description,
        default_value: new_default_value,
        source_extension: new_source,
    } = new;

    let mut attributes: Vec<AttributeChange> = Vec::new();
    attributes.extend(scalar(
        Scalar::FieldType,
        Some(old_type.as_str().to_string()),
        Some(new_type.as_str().to_string()),
    ));
    attributes.extend(scalar(
        Scalar::Required,
        flag(*old_required),
        flag(*new_required),
    ));
    // A type change already says the field's values changed: an enum's
    // values are present exactly for an enum.
    if old_type == new_type {
        attributes.extend(members(
            Members::EnumValues,
            old_enum_values.as_deref().unwrap_or_default(),
            new_enum_values.as_deref().unwrap_or_default(),
        ));
    }
    attributes.extend(scalar(Scalar::Edge, old_edge.clone(), new_edge.clone()));
    attributes.extend(scalar(
        Scalar::TargetKind,
        old_target_kind.clone(),
        new_target_kind.clone(),
    ));
    attributes.extend(scalar(
        Scalar::Description,
        old_description.clone(),
        new_description.clone(),
    ));
    attributes.extend(scalar(
        Scalar::DefaultValue,
        old_default_value.clone(),
        new_default_value.clone(),
    ));
    attributes.extend(scalar(
        Scalar::SourceExtension,
        Some(old_source.clone()),
        Some(new_source.clone()),
    ));
    for change in attributes {
        changes.push(SchemaChange::Field {
            kind: kind.to_string(),
            field: field.to_string(),
            change,
        });
    }
}

fn diff_edge_type(
    label: &str,
    old: &SchemaEdgeType,
    new: &SchemaEdgeType,
    changes: &mut Vec<SchemaChange>,
) {
    let SchemaEdgeType {
        label: _,
        source_extension: old_source,
        source_kinds: old_source_kinds,
        target_kinds: old_target_kinds,
    } = old;
    let SchemaEdgeType {
        label: _,
        source_extension: new_source,
        source_kinds: new_source_kinds,
        target_kinds: new_target_kinds,
    } = new;

    let mut attributes: Vec<AttributeChange> = Vec::new();
    attributes.extend(members(
        Members::SourceKinds,
        old_source_kinds.as_deref().unwrap_or_default(),
        new_source_kinds.as_deref().unwrap_or_default(),
    ));
    attributes.extend(members(
        Members::TargetKinds,
        old_target_kinds.as_deref().unwrap_or_default(),
        new_target_kinds.as_deref().unwrap_or_default(),
    ));
    attributes.extend(scalar(
        Scalar::SourceExtension,
        Some(old_source.clone()),
        Some(new_source.clone()),
    ));
    for change in attributes {
        changes.push(SchemaChange::EdgeType {
            label: label.to_string(),
            change,
        });
    }
}

/// `previous` moved by the largest bump among `changes`; `previous` itself
/// when there is none.
pub(super) fn bumped(previous: &SchemaVersion, changes: &[SchemaChange]) -> SchemaVersion {
    match changes.iter().map(SchemaChange::bump).max() {
        None => previous.clone(),
        Some(Bump::Major) => SchemaVersion::new(previous.major + 1, 0, 0),
        Some(Bump::Minor) => SchemaVersion::new(previous.major, previous.minor + 1, 0),
        Some(Bump::Patch) => SchemaVersion::new(previous.major, previous.minor, previous.patch + 1),
    }
}
