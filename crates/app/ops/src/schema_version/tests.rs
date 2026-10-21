use super::diff::{bumped, diff};
use super::*;
use specforge_emitter::{
    GraphProtocolSchema, SchemaEdgeType, SchemaEntityKind, SchemaExtensionInfo, SchemaField,
    SchemaVersion,
};
use specforge_protocol_types::FieldType;
use specforge_registry::PlainDisk;
use specforge_test::prelude::*;

fn sample_schema() -> GraphProtocolSchema {
    GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 2, 3),
        extensions: vec![SchemaExtensionInfo {
            name: "@specforge/software".to_string(),
            version: "1.0.0".to_string(),
        }],
        entity_kinds: vec![
            SchemaEntityKind {
                name: "behavior".to_string(),
                source_extension: "@specforge/software".to_string(),
                testable: true,
                dot_color: None,
                fields: vec![SchemaField {
                    name: "contract".to_string(),
                    field_type: FieldType::String,
                    required: false,
                    enum_values: None,
                    edge: None,
                    target_kind: None,
                    description: None,
                    default_value: None,
                    source_extension: "@specforge/software".to_string(),
                }],
            },
            SchemaEntityKind {
                name: "feature".to_string(),
                source_extension: "@specforge/product".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
        ],
        edge_types: vec![SchemaEdgeType {
            label: "implements".to_string(),
            source_extension: "@specforge/software".to_string(),
            source_kinds: Some(vec!["behavior".to_string()]),
            target_kinds: Some(vec!["feature".to_string()]),
        }],
    }
}

fn optional_field(name: &str) -> SchemaField {
    SchemaField {
        name: name.to_string(),
        field_type: FieldType::String,
        required: false,
        enum_values: None,
        edge: None,
        target_kind: None,
        description: None,
        default_value: None,
        source_extension: "@specforge/software".to_string(),
    }
}

fn breaking(changes: &[SchemaChange]) -> bool {
    changes.iter().any(|c| c.bump() == Bump::Major)
}

fn additions(changes: &[SchemaChange]) -> bool {
    changes.iter().any(|c| c.bump() == Bump::Minor)
}

// ===========================================================================
// Every attribute is compared and classified (ADR 0067 D2)
// ===========================================================================

/// A schema holding every attribute a change can touch: two extensions, a
/// kind with a DOT colour and an enum field with a default and a description,
/// a field that writes an edge type, and an edge type with two ends each.
fn rich_schema() -> GraphProtocolSchema {
    let software = "@specforge/software".to_string();
    GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 2, 3),
        extensions: vec![
            SchemaExtensionInfo {
                name: software.clone(),
                version: "1.0.0".to_string(),
            },
            SchemaExtensionInfo {
                name: "@specforge/product".to_string(),
                version: "1.0.0".to_string(),
            },
        ],
        entity_kinds: vec![
            SchemaEntityKind {
                name: "behavior".to_string(),
                source_extension: software.clone(),
                testable: true,
                dot_color: Some("#111".to_string()),
                fields: vec![
                    SchemaField {
                        required: true,
                        ..optional_field("contract")
                    },
                    SchemaField {
                        field_type: FieldType::Enum,
                        enum_values: Some(vec!["a".to_string(), "b".to_string()]),
                        description: Some("d".to_string()),
                        default_value: Some("a".to_string()),
                        ..optional_field("status")
                    },
                    SchemaField {
                        edge: Some("OwnedBy".to_string()),
                        target_kind: Some("person".to_string()),
                        ..optional_field("owner")
                    },
                ],
            },
            SchemaEntityKind {
                name: "feature".to_string(),
                source_extension: "@specforge/product".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
        ],
        edge_types: vec![SchemaEdgeType {
            label: "E".to_string(),
            source_extension: software,
            source_kinds: Some(vec!["behavior".to_string(), "feature".to_string()]),
            target_kinds: Some(vec!["feature".to_string(), "behavior".to_string()]),
        }],
    }
}

fn behavior(schema: &mut GraphProtocolSchema) -> &mut SchemaEntityKind {
    schema
        .entity_kinds
        .iter_mut()
        .find(|k| k.name == "behavior")
        .unwrap()
}

fn field<'a>(schema: &'a mut GraphProtocolSchema, name: &str) -> &'a mut SchemaField {
    behavior(schema)
        .fields
        .iter_mut()
        .find(|f| f.name == name)
        .unwrap()
}

mod admit;
mod attributes;
mod diff;
mod record;
mod version;
