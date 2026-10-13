use specforge_common::{SourceSpan, Sym};
use specforge_emitter::json::emit_json;
use specforge_emitter::{
    EmitFormat, EmitOptions, EmitterError, GraphProtocolSchema, SchemaEdgeType, SchemaEntityKind,
    SchemaExtensionInfo, SchemaField, SchemaMigration, SchemaMigrationChange, SchemaVersion,
    SchemaVersionError, compute_schema_version, diff_schemas, diff_schemas_optional,
    generate_schema, negotiate_version, publish,
};
use specforge_graph::{Edge, FieldValue, Graph, Node};
use specforge_parser::{EntityId, EntityKind, FieldMap};
use specforge_protocol_types::{
    EdgeTypeDescriptor, EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor,
};
use specforge_registry::{
    FieldRegistry, FieldRegistryEntry, FieldType, RegistryBuild, build_registries,
};
use specforge_test::prelude::*;
use std::sync::Arc;

/// `graph` exported as `format` with `schema` attached: embedded, or
/// referenced when scoped (`emit`, ADR 0007).
fn with_schema(
    graph: &Graph,
    format: EmitFormat,
    scope: Option<&str>,
    schema: &GraphProtocolSchema,
) -> Result<String, EmitterError> {
    specforge_emitter::emit(
        graph,
        &EmitOptions {
            format,
            scope,
            schema: Some(schema),
            ..EmitOptions::default()
        },
    )
}

fn span() -> SourceSpan {
    SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 0,
    }
}

fn node(id: &str, kind: &str, title: Option<&str>) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: title.map(|s| s.to_string()),
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
    }
}

/// A kind declaration: `name` is its keyword; `testable` kinds accept
/// `verify`.
fn kind_decl(name: &str, testable: bool, fields: Vec<FieldDescriptor>) -> EntityKindDescriptor {
    EntityKindDescriptor {
        name: name.to_string(),
        keyword: Some(name.to_string()),
        testable,
        supports_verify: testable,
        fields,
        ..Default::default()
    }
}

/// An edge type declaration; a side given as `None` is open.
fn edge_decl(label: &str, src: Option<&str>, tgt: Option<&str>) -> EdgeTypeDescriptor {
    EdgeTypeDescriptor {
        label: label.to_string(),
        source_kind: src.map(|s| s.to_string()),
        target_kind: tgt.map(|s| s.to_string()),
        ..Default::default()
    }
}

/// A field declaration of `ft`.
fn field_decl(name: &str, ft: FieldType, required: bool) -> FieldDescriptor {
    FieldDescriptor {
        name: name.to_string(),
        field_type: ft.as_str().to_string(),
        required,
        ..Default::default()
    }
}

/// An enum field declaration.
fn enum_decl(name: &str, values: &[&str], required: bool) -> FieldDescriptor {
    FieldDescriptor {
        enum_values: values.iter().map(|v| v.to_string()).collect(),
        ..field_decl(name, FieldType::Enum, required)
    }
}

/// A reference-list field declaration writing `edge` towards `target`.
fn edge_writer_decl(name: &str, edge: &str, target: &str) -> FieldDescriptor {
    FieldDescriptor {
        edge: Some(edge.to_string()),
        target_kind: Some(target.to_string()),
        ..field_decl(name, FieldType::ReferenceList, false)
    }
}

/// The extension `name` (version 1.0.0) declaring `entities` and `edges`.
fn declaration(
    name: &str,
    entities: Vec<EntityKindDescriptor>,
    edges: Vec<EdgeTypeDescriptor>,
) -> ExtensionDeclaration {
    let mut declaration = ExtensionDeclaration::default();
    declaration.handshake.name = name.to_string();
    declaration.handshake.version = "1.0.0".to_string();
    declaration.entities = entities;
    declaration.edges = edges;
    declaration
}

/// The sample project's registries: the software extension's `behavior`
/// (a `contract` string and an enum `status`) and its edge type
/// `implements` to the product extension's `feature`.
fn software_and_product() -> RegistryBuild {
    build_registries(vec![
        declaration(
            "@specforge/software",
            vec![kind_decl(
                "behavior",
                true,
                vec![
                    field_decl("contract", FieldType::String, false),
                    enum_decl("status", &["draft", "done"], false),
                ],
            )],
            vec![edge_decl("implements", Some("behavior"), Some("feature"))],
        ),
        declaration(
            "@specforge/product",
            vec![kind_decl("feature", false, vec![])],
            vec![],
        ),
    ])
}

fn make_field_entry(kind: &str, field: &str, ft: FieldType, required: bool) -> FieldRegistryEntry {
    FieldRegistryEntry::new(
        kind,
        "@specforge/software",
        specforge_registry::FieldDescriptor {
            name: field.to_string(),
            field_type: ft.as_str().to_string(),
            required,
            ..Default::default()
        },
    )
    .unwrap()
}

/// `entry` with its declaration changed by `change`.
fn amended(
    entry: FieldRegistryEntry,
    change: impl FnOnce(&mut specforge_registry::FieldDescriptor),
) -> FieldRegistryEntry {
    let mut declared = entry.declared().clone();
    change(&mut declared);
    FieldRegistryEntry::new(entry.kind_name(), entry.source_extension(), declared).unwrap()
}

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

// ===========================================================================
// Slice 1: Schema Types
// ===========================================================================

// B:generate_schema_from_registries — verify unit "SchemaVersion Display produces MAJOR.MINOR.PATCH"
#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "SchemaVersion Display"
)]
fn schema_version_display() {
    assert_eq!(SchemaVersion::new(1, 0, 0).to_string(), "1.0.0");
    assert_eq!(SchemaVersion::new(2, 3, 4).to_string(), "2.3.4");
    let mut v = SchemaVersion::new(1, 0, 0);
    v.label = Some("beta".to_string());
    assert_eq!(v.to_string(), "1.0.0-beta");
}

// B:generate_schema_from_registries — verify unit "SchemaVersion FromStr round-trips"
#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "SchemaVersion FromStr"
)]
fn schema_version_from_str() {
    let v: SchemaVersion = "1.2.3".parse().unwrap();
    assert_eq!(v, SchemaVersion::new(1, 2, 3));
    assert!(v.label.is_none());

    let v2: SchemaVersion = "2.0.0-alpha".parse().unwrap();
    assert_eq!(v2.major, 2);
    assert_eq!(v2.label, Some("alpha".to_string()));

    assert!("invalid".parse::<SchemaVersion>().is_err());
    assert!("1.2".parse::<SchemaVersion>().is_err());
}

// B:generate_schema_from_registries — verify unit "SchemaVersion Ord compares correctly"
#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "SchemaVersion Ord"
)]
fn schema_version_ord() {
    let v1 = SchemaVersion::new(1, 0, 0);
    let v2 = SchemaVersion::new(1, 1, 0);
    let v3 = SchemaVersion::new(2, 0, 0);
    assert!(v1 < v2);
    assert!(v2 < v3);
    assert!(v1 < v3);
    assert_eq!(v1, SchemaVersion::new(1, 0, 0));
}

#[test]
fn empty_schema_serde_round_trip() {
    let schema = GraphProtocolSchema::empty();
    let json = serde_json::to_string(&schema).unwrap();
    let deserialized: GraphProtocolSchema = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, schema);
    assert!(deserialized.entity_kinds.is_empty());
    assert!(deserialized.edge_types.is_empty());
    assert!(deserialized.extensions.is_empty());
}

// B:generate_schema_from_registries — verify unit "populated schema round-trips through serde"
#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "populated schema serde round-trip"
)]
fn populated_schema_serde_round_trip() {
    let schema = sample_schema();
    let json = serde_json::to_string_pretty(&schema).unwrap();
    let deserialized: GraphProtocolSchema = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, schema);
}

// ===========================================================================
// Slice 2: Generate Schema from Registries
// ===========================================================================

// B:generate_schema_from_registries — verify unit "generate from registries with kinds and edges"
#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "schema includes all registered entity kinds"
)]
fn generate_schema_includes_kinds_edges_fields() {
    let build = software_and_product();

    let schema = generate_schema(&build);

    assert_eq!(schema.entity_kinds.len(), 2);
    assert_eq!(schema.entity_kinds[0].name, "behavior");
    assert!(schema.entity_kinds[0].testable);
    assert_eq!(schema.entity_kinds[0].fields.len(), 2);
    assert_eq!(schema.entity_kinds[1].name, "feature");
    assert!(!schema.entity_kinds[1].testable);
    assert_eq!(schema.entity_kinds[1].fields.len(), 0);

    assert_eq!(schema.edge_types.len(), 1);
    assert_eq!(schema.edge_types[0].label, "implements");
    assert_eq!(
        schema.edge_types[0].source_kinds,
        Some(vec!["behavior".to_string()])
    );
    assert_eq!(
        schema.edge_types[0].target_kinds,
        Some(vec!["feature".to_string()])
    );

    // The schema's extensions are the build's.
    let loaded: Vec<(&str, &str)> = build.extension_info().collect();
    let listed: Vec<(&str, &str)> = schema
        .extensions
        .iter()
        .map(|e| (e.name.as_str(), e.version.as_str()))
        .collect();
    assert_eq!(
        loaded,
        [
            ("@specforge/software", "1.0.0"),
            ("@specforge/product", "1.0.0")
        ]
    );
    assert_eq!(listed, loaded);
}

// B:generate_schema_from_registries — verify unit "zero-extension registries produce valid empty schema"
#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "zero extensions produces valid empty schema"
)]
fn generate_schema_empty_registries() {
    let build = build_registries(vec![]);

    let schema = generate_schema(&build);
    assert!(schema.entity_kinds.is_empty());
    assert!(schema.edge_types.is_empty());
    assert!(schema.extensions.is_empty());
    assert_eq!(schema.schema_version, SchemaVersion::new(1, 0, 0));
}

// B:generate_schema_from_registries — verify unit "field types map correctly"
#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "schema fields match FieldRegistry entries"
)]
fn generate_schema_field_type_mapping() {
    let build = build_registries(vec![declaration(
        "@specforge/software",
        vec![kind_decl(
            "behavior",
            true,
            vec![
                field_decl("contract", FieldType::String, false),
                field_decl("priority", FieldType::Integer, false),
                field_decl("active", FieldType::Bool, false),
                field_decl("invariants", FieldType::ReferenceList, false),
                enum_decl("status", &["draft", "done"], false),
            ],
        )],
        vec![],
    )]);

    let schema = generate_schema(&build);

    let kind = &schema.entity_kinds[0];
    assert_eq!(kind.fields.len(), 5);

    let find_field = |name: &str| kind.fields.iter().find(|f| f.name == name).unwrap();
    assert_eq!(find_field("contract").field_type, FieldType::String);
    assert_eq!(find_field("priority").field_type, FieldType::Integer);
    assert_eq!(find_field("active").field_type, FieldType::Bool);
    assert_eq!(
        find_field("invariants").field_type,
        FieldType::ReferenceList
    );
    assert_eq!(find_field("status").field_type, FieldType::Enum);
    assert_eq!(
        find_field("status").enum_values,
        Some(vec!["draft".to_string(), "done".to_string()])
    );
}

// B:generate_schema_from_registries — verify unit "entity_kinds sorted by name"
#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "schema includes all registered edge types"
)]
fn generate_schema_deterministic_sort() {
    let build = build_registries(vec![declaration(
        "@specforge/software",
        vec![
            kind_decl("type", false, vec![]),
            kind_decl("behavior", true, vec![]),
            kind_decl("invariant", true, vec![]),
        ],
        vec![
            edge_decl("implements", None, None),
            edge_decl("enforces", None, None),
        ],
    )]);

    let schema = generate_schema(&build);

    let kind_names: Vec<&str> = schema
        .entity_kinds
        .iter()
        .map(|k| k.name.as_str())
        .collect();
    assert_eq!(kind_names, vec!["behavior", "invariant", "type"]);

    let edge_labels: Vec<&str> = schema.edge_types.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(edge_labels, vec!["enforces", "implements"]);
}

// ===========================================================================
// Slice 3: Embed Schema in Export
// ===========================================================================

// B:embed_schema_in_export — a graph export with a schema is format 2.0
#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "format_version set to 2.0 with schema"
)]
fn a_graph_export_with_a_schema_is_format_2_0() {
    let graph = Graph::new();
    let schema = GraphProtocolSchema::empty();
    let json = with_schema(&graph, EmitFormat::Json, None, &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["format_version"], "2.0");
    assert!(parsed["schema"].is_object());
    assert!(parsed["schema_version"].is_string());
}

// B:embed_schema_in_export — a graph export embeds the schema as a top-level key
#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "schema embedded as top-level key in full JSON export"
)]
fn a_graph_export_embeds_the_schema() {
    let mut graph = Graph::new();
    graph.add_node(node("alpha", "behavior", Some("Alpha")));
    let schema = sample_schema();
    let json = with_schema(&graph, EmitFormat::Json, None, &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(parsed["schema"]["entity_kinds"].is_array());
    assert!(parsed["schema"]["edge_types"].is_array());
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 1);
}

#[test]
fn existing_emit_json_has_no_schema_key() {
    let graph = Graph::new();
    let json = emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.get("schema").is_none());
    assert_eq!(parsed["format_version"], "1.0");
}

// B:embed_schema_in_export — a context export with a schema is format 2.0
#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "format_version set to 2.0 with schema"
)]
fn a_context_export_with_a_schema_is_format_2_0() {
    let graph = Graph::new();
    let schema = GraphProtocolSchema::empty();
    let json = with_schema(&graph, EmitFormat::Context, None, &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["format_version"], "2.0");
    assert!(parsed["schema"].is_object());
}

// B:embed_schema_in_export — a brief export with a schema is format 2.0
#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "format_version set to 2.0 with schema"
)]
fn a_brief_export_with_a_schema_is_format_2_0() {
    let graph = Graph::new();
    let schema = GraphProtocolSchema::empty();
    let json = with_schema(&graph, EmitFormat::Brief, None, &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["format_version"], "2.0");
    assert!(parsed["schema"].is_object());
}

// B:embed_schema_in_export — verify unit "schema_version in V2 matches schema object"
#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "output includes schema_version field"
)]
fn the_export_version_is_the_schema_version() {
    let graph = Graph::new();
    let schema = sample_schema();
    let json = with_schema(&graph, EmitFormat::Json, None, &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let top_level = parsed["schema_version"].as_str().unwrap();
    assert_eq!(top_level, "1.2.3");
}

// ===========================================================================
// Slice 4: Schema Diffing
// ===========================================================================

// B:detect_breaking_schema_changes — verify unit "removed kind is breaking"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "removed entity kind detected as breaking"
)]
fn diff_removed_kind_is_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.entity_kinds.retain(|k| k.name != "feature");

    let migration = diff_schemas(&old, &new);
    assert!(migration.has_breaking_changes());
    assert!(
        migration
            .changes
            .iter()
            .any(|c| matches!(c, SchemaMigrationChange::KindRemoved(name) if name == "feature"))
    );
}

// B:detect_breaking_schema_changes — verify unit "added kind is non-breaking"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "new entity kind detected as non-breaking"
)]
fn diff_added_kind_is_non_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.entity_kinds.push(SchemaEntityKind {
        name: "event".to_string(),
        source_extension: "@specforge/software".to_string(),
        testable: true,
        dot_color: None,
        fields: vec![],
    });

    let migration = diff_schemas(&old, &new);
    assert!(!migration.has_breaking_changes());
    assert!(migration.has_additions());
}

// B:detect_breaking_schema_changes — verify unit "added optional field is non-breaking"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "added optional field detected as non-breaking"
)]
fn diff_added_optional_field_non_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.entity_kinds[0].fields.push(SchemaField {
        name: "description".to_string(),
        field_type: FieldType::String,
        required: false,
        enum_values: None,
        edge: None,
        target_kind: None,
        description: None,
        default_value: None,
        source_extension: "@specforge/software".to_string(),
    });

    let migration = diff_schemas(&old, &new);
    assert!(!migration.has_breaking_changes());
    assert!(migration.has_additions());
}

// B:detect_breaking_schema_changes — verify unit "added required field is breaking"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "new required field detected as breaking"
)]
fn diff_added_required_field_is_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.entity_kinds[0].fields.push(SchemaField {
        name: "severity".to_string(),
        field_type: FieldType::String,
        required: true,
        enum_values: None,
        edge: None,
        target_kind: None,
        description: None,
        default_value: None,
        source_extension: "@specforge/software".to_string(),
    });

    let migration = diff_schemas(&old, &new);
    assert!(migration.has_breaking_changes());
}

// B:detect_breaking_schema_changes — verify unit "removed field is breaking"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "removed field is breaking"
)]
fn diff_removed_field_is_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.entity_kinds[0].fields.clear();

    let migration = diff_schemas(&old, &new);
    assert!(migration.has_breaking_changes());
    assert!(migration.changes.iter().any(|c| matches!(c, SchemaMigrationChange::FieldRemoved { kind, field } if kind == "behavior" && field == "contract")));
}

// B:detect_breaking_schema_changes — verify unit "removed edge is breaking"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "removed edge type detected as breaking"
)]
fn diff_removed_edge_is_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.edge_types.clear();

    let migration = diff_schemas(&old, &new);
    assert!(migration.has_breaking_changes());
}

// B:detect_breaking_schema_changes — verify unit "added edge is non-breaking"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "new edge type detected as non-breaking"
)]
fn diff_added_edge_non_breaking() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.edge_types.push(SchemaEdgeType {
        label: "enforces".to_string(),
        source_extension: "@specforge/software".to_string(),
        source_kinds: None,
        target_kinds: None,
    });

    let migration = diff_schemas(&old, &new);
    assert!(!migration.has_breaking_changes());
    assert!(migration.has_additions());
}

// B:detect_breaking_schema_changes — verify unit "no previous schema means all non-breaking"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "no previous schema treats all changes as non-breaking"
)]
fn diff_no_previous_schema_all_non_breaking() {
    let new = sample_schema();
    let migration = diff_schemas_optional(None, &new);
    assert!(!migration.has_breaking_changes());
    assert!(migration.has_additions());
}

// B:detect_breaking_schema_changes — verify unit "identical schemas have no changes"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "identical schemas no changes"
)]
fn diff_identical_schemas_no_changes() {
    let old = sample_schema();
    let new = sample_schema();
    let migration = diff_schemas(&old, &new);
    assert!(migration.is_empty());
    assert!(!migration.has_breaking_changes());
    assert!(!migration.has_additions());
}

// ===========================================================================
// Slice 5: Compute Schema Version
// ===========================================================================

// B:compute_schema_version — verify unit "no cache yields 1.0.0"
#[specforge_test(
    behavior = "compute_schema_version",
    verify = "first compilation without cache produces version 1.0.0"
)]
fn compute_version_no_cache() {
    let migration = SchemaMigration { changes: vec![] };
    let version = compute_schema_version(&migration, None);
    assert_eq!(version, SchemaVersion::new(1, 0, 0));
}

// B:compute_schema_version — verify unit "new kind bumps minor"
#[specforge_test(
    behavior = "compute_schema_version",
    verify = "new entity kind triggers minor version bump"
)]
fn compute_version_new_kind_bumps_minor() {
    let migration = SchemaMigration {
        changes: vec![SchemaMigrationChange::KindAdded("event".to_string())],
    };
    let version = compute_schema_version(&migration, Some(&SchemaVersion::new(1, 2, 3)));
    assert_eq!(version, SchemaVersion::new(1, 3, 0));
}

// B:compute_schema_version — verify unit "removed kind bumps major"
#[specforge_test(
    behavior = "compute_schema_version",
    verify = "removed entity kind triggers major version bump"
)]
fn compute_version_removed_kind_bumps_major() {
    let migration = SchemaMigration {
        changes: vec![SchemaMigrationChange::KindRemoved("event".to_string())],
    };
    let version = compute_schema_version(&migration, Some(&SchemaVersion::new(1, 2, 3)));
    assert_eq!(version, SchemaVersion::new(2, 0, 0));
}

// B:compute_schema_version — verify unit "no changes returns previous unchanged"
#[specforge_test(
    behavior = "compute_schema_version",
    verify = "no changes returns previous"
)]
fn compute_version_no_changes_returns_previous() {
    let migration = SchemaMigration { changes: vec![] };
    let version = compute_schema_version(&migration, Some(&SchemaVersion::new(3, 1, 4)));
    assert_eq!(version, SchemaVersion::new(3, 1, 4));
}

// B:compute_schema_version — verify unit "breaking + non-breaking = major bump"
#[specforge_test(
    behavior = "compute_schema_version",
    verify = "removed entity kind triggers major version bump"
)]
fn compute_version_mixed_changes_major_wins() {
    let migration = SchemaMigration {
        changes: vec![
            SchemaMigrationChange::KindAdded("event".to_string()),
            SchemaMigrationChange::KindRemoved("legacy".to_string()),
        ],
    };
    let version = compute_schema_version(&migration, Some(&SchemaVersion::new(1, 5, 2)));
    assert_eq!(version, SchemaVersion::new(2, 0, 0));
}

// ===========================================================================
// Slice 6: Version Negotiation
// ===========================================================================

// B:negotiate_schema_version — verify unit "in-range version succeeds"
#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "compatible version within range is resolved"
)]
fn negotiate_in_range_succeeds() {
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    let requested = SchemaVersion::new(1, 3, 0);

    let result = negotiate_version(&requested, &min, &max);
    assert!(result.is_ok());
    let compat = result.unwrap();
    assert_eq!(compat.requested, requested);
    assert_eq!(compat.resolved, requested);
}

// B:negotiate_schema_version — verify unit "exact min boundary succeeds"
#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "compatible version within range is resolved"
)]
fn negotiate_exact_min_succeeds() {
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    let result = negotiate_version(&min, &min, &max);
    assert!(result.is_ok());
}

// B:negotiate_schema_version — verify unit "exact max boundary succeeds"
#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "compatible version within range is resolved"
)]
fn negotiate_exact_max_succeeds() {
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    let result = negotiate_version(&max, &min, &max);
    assert!(result.is_ok());
}

// B:negotiate_schema_version — verify unit "out-of-range version fails with E027"
#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "incompatible version produces E027 with supported range"
)]
fn negotiate_out_of_range_fails() {
    let min = SchemaVersion::new(1, 2, 0);
    let max = SchemaVersion::new(1, 5, 0);
    let requested = SchemaVersion::new(1, 1, 0);

    let err = negotiate_version(&requested, &min, &max).unwrap_err();
    assert_eq!(
        err.to_string(),
        "E027: requested schema version 1.1.0 is out of range [1.2.0, 1.5.0]"
    );
    assert_eq!(
        (err.min, err.max),
        (SchemaVersion::new(1, 2, 0), SchemaVersion::new(1, 5, 0))
    );
}

// B:negotiate_schema_version — verify unit "different major version fails"
#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "incompatible version produces E027 with supported range"
)]
fn negotiate_different_major_fails() {
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    let requested = SchemaVersion::new(2, 0, 0);

    let err = negotiate_version(&requested, &min, &max).unwrap_err();
    assert_eq!(
        err.to_string(),
        "E027: requested schema version 2.0.0 has incompatible major version \
         (supported range [1.0.0, 1.5.0])"
    );
    assert_eq!(err.requested, requested);
}

// B:negotiate_schema_version — verify unit "SchemaVersionError Display includes E027"
#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "incompatible version produces E027 with supported range"
)]
fn schema_version_error_display() {
    // Below the supported minimum, in the same major version.
    let err = negotiate_version(
        &SchemaVersion::new(1, 0, 9),
        &SchemaVersion::new(1, 1, 0),
        &SchemaVersion::new(1, 4, 2),
    )
    .unwrap_err();
    let shown = err.to_string();
    assert!(shown.starts_with("E027: "), "{shown}");
    assert!(shown.ends_with("[1.1.0, 1.4.2]"), "{shown}");

    // A hand-built error displays its own reason after the code.
    let built = SchemaVersionError {
        requested: SchemaVersion::new(2, 0, 0),
        min: SchemaVersion::new(1, 0, 0),
        max: SchemaVersion::new(1, 5, 0),
        reason: "incompatible".to_string(),
    };
    assert_eq!(built.to_string(), "E027: incompatible");
}

// ===========================================================================
// Slice 9: Publish JSON Schema
// ===========================================================================

/// The kinds a published document describes: the `node_<kind>` branches of a
/// graph document, the node `kind` enum of a context or brief document.
fn published_kinds(document: &serde_json::Value) -> std::collections::BTreeSet<String> {
    let items = &document["properties"]["nodes"]["items"];
    if let Some(branches) = items["oneOf"].as_array() {
        return branches
            .iter()
            .map(|branch| {
                branch["$ref"]
                    .as_str()
                    .and_then(|r| r.strip_prefix("#/$defs/node_"))
                    .unwrap_or_else(|| panic!("{branch}"))
                    .to_string()
            })
            .collect();
    }
    items["properties"]["kind"]["enum"]
        .as_array()
        .map(|kinds| {
            kinds
                .iter()
                .map(|kind| kind.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

// Not linked: only a few top-level keys are checked here.
// schema_publish_produces_json_schema_draft in specforge-cli's e2e_schema
// tests checks every keyword of the published schema against draft 2020-12.
#[test]
fn publish_json_schema_valid() {
    let schema = sample_schema();
    let parsed = publish(&schema, EmitFormat::Json).unwrap();

    assert_eq!(
        parsed["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert!(parsed["title"].is_string());
    assert_eq!(parsed["type"], "object");
}

// B:publish_schema_specification — verify unit "all kinds in node kind enum"
#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "published schema describes all registered entity kinds"
)]
fn publish_json_schema_kinds_in_enum() {
    let schema = sample_schema();
    let parsed = publish(&schema, EmitFormat::Json).unwrap();

    let kinds = published_kinds(&parsed);
    assert!(kinds.contains("behavior"));
    assert!(kinds.contains("feature"));
    // The kinds an export holds beyond the declared ones: the structural
    // `spec` and `ref`.
    assert!(kinds.contains("spec") && kinds.contains("ref"), "{kinds:?}");
}

// B:publish_schema_specification — verify unit "all edge labels in edge label enum"
#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "published schema describes all edge types"
)]
fn publish_json_schema_edge_labels_in_enum() {
    let mut schema = sample_schema();
    schema.entity_kinds[0].fields.push(SchemaField {
        name: "features".to_string(),
        field_type: FieldType::ReferenceList,
        required: false,
        enum_values: None,
        edge: Some("implements".to_string()),
        target_kind: Some("feature".to_string()),
        description: None,
        default_value: None,
        source_extension: "@specforge/software".to_string(),
    });
    let parsed = publish(&schema, EmitFormat::Json).unwrap();

    // The embedded schema block's edge types are the registered ones.
    assert_eq!(
        parsed["$defs"]["GraphProtocolSchema"]["properties"]["edge_types"]["items"]["properties"]["label"],
        serde_json::json!({ "type": "string", "enum": ["implements"] })
    );
    // A graph edge carries the declaring field's name: an open string whose
    // examples are the fields that declare an edge type.
    let label = &parsed["$defs"]["Edge"]["properties"]["label"];
    assert_eq!(label["type"], "string");
    assert!(label.get("enum").is_none(), "{label}");
    assert_eq!(label["examples"], serde_json::json!(["features"]));
}

// B:publish_schema_specification — verify unit "required properties present"
#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "published schema requires the Graph Protocol top-level properties"
)]
fn publish_json_schema_required_properties() {
    let schema = sample_schema();
    let parsed = publish(&schema, EmitFormat::Json).unwrap();

    assert_eq!(
        parsed["required"],
        serde_json::json!(["format_version", "schema_version", "nodes", "edges"])
    );
    // Context and brief exports carry a format_version only with a schema.
    for format in [EmitFormat::Context, EmitFormat::Brief] {
        assert_eq!(
            publish(&schema, format).unwrap()["required"],
            serde_json::json!(["schema_version", "nodes", "edges"]),
            "{format:?}"
        );
    }
}

#[test]
fn publish_json_schema_empty_schema() {
    let schema = GraphProtocolSchema::empty();
    let parsed = publish(&schema, EmitFormat::Json).unwrap();

    assert_eq!(
        parsed["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    // The nodes stay as the export document derives them when no kind is
    // registered: any kind, no per-kind branches.
    let kind_prop = &parsed["properties"]["nodes"]["items"]["properties"]["kind"];
    assert!(kind_prop.get("enum").is_none());
    assert_eq!(kind_prop["type"], "string");
    assert!(
        parsed["properties"]["nodes"]["items"]
            .get("oneOf")
            .is_none()
    );
}

// Not linked: a title says nothing about validity (see
// publish_json_schema_valid).
#[test]
fn publish_json_schema_has_title() {
    let schema = sample_schema();
    let parsed = publish(&schema, EmitFormat::Json).unwrap();
    assert_eq!(parsed["title"], "SpecForge Graph Protocol");
}

// ===========================================================================
// Integration: End-to-end
// ===========================================================================

// B:generate_schema_from_registries — verify integration "full pipeline: registries → schema → embed → diff"
#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "schema includes all registered entity kinds"
)]
fn full_pipeline_registries_to_schema_to_embed() {
    // Build registries
    let build = software_and_product();

    // Generate
    let schema = generate_schema(&build);
    assert_eq!(schema.entity_kinds.len(), 2);

    // Embed
    let mut graph = Graph::new();
    graph.add_node(node("alpha", "behavior", Some("Alpha")));
    graph.add_node(node("beta", "feature", Some("Beta")));
    graph.add_edge(Edge {
        source: Sym::new("alpha"),
        target: Sym::new("beta"),
        label: Sym::new("implements"),
    });

    let json = with_schema(&graph, EmitFormat::Json, None, &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["format_version"], "2.0");
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["edges"].as_array().unwrap().len(), 1);
    assert_eq!(
        parsed["schema"]["entity_kinds"].as_array().unwrap().len(),
        2
    );

    // Diff with empty
    let migration = diff_schemas_optional(None, &schema);
    assert!(!migration.has_breaking_changes());

    // Version
    let version = compute_schema_version(&migration, None);
    assert_eq!(version, SchemaVersion::new(1, 0, 0));
}

// B:compute_schema_version — verify unit "new edge bumps minor"
#[specforge_test(
    behavior = "compute_schema_version",
    verify = "new edge type triggers minor version bump"
)]
fn compute_version_new_edge_bumps_minor() {
    let migration = SchemaMigration {
        changes: vec![SchemaMigrationChange::EdgeAdded("enforces".to_string())],
    };
    let version = compute_schema_version(&migration, Some(&SchemaVersion::new(1, 0, 0)));
    assert_eq!(version, SchemaVersion::new(1, 1, 0));
}

// B:compute_schema_version — verify unit "removed edge bumps major"
#[specforge_test(
    behavior = "compute_schema_version",
    verify = "removed edge type triggers major version bump"
)]
fn compute_version_removed_edge_bumps_major() {
    let migration = SchemaMigration {
        changes: vec![SchemaMigrationChange::EdgeRemoved("enforces".to_string())],
    };
    let version = compute_schema_version(&migration, Some(&SchemaVersion::new(1, 0, 0)));
    assert_eq!(version, SchemaVersion::new(2, 0, 0));
}

// B:compute_schema_version — verify unit "added optional field bumps minor"
#[specforge_test(
    behavior = "compute_schema_version",
    verify = "added optional field bumps minor"
)]
fn compute_version_added_optional_field_bumps_minor() {
    let migration = SchemaMigration {
        changes: vec![SchemaMigrationChange::FieldAdded {
            kind: "behavior".to_string(),
            field: "description".to_string(),
            required: false,
        }],
    };
    let version = compute_schema_version(&migration, Some(&SchemaVersion::new(1, 0, 0)));
    assert_eq!(version, SchemaVersion::new(1, 1, 0));
}

// B:compute_schema_version — verify unit "added required field bumps major"
#[specforge_test(
    behavior = "compute_schema_version",
    verify = "new required field triggers major version bump"
)]
fn compute_version_added_required_field_bumps_major() {
    let migration = SchemaMigration {
        changes: vec![SchemaMigrationChange::FieldAdded {
            kind: "behavior".to_string(),
            field: "severity".to_string(),
            required: true,
        }],
    };
    let version = compute_schema_version(&migration, Some(&SchemaVersion::new(1, 0, 0)));
    assert_eq!(version, SchemaVersion::new(2, 0, 0));
}

// B:compute_schema_version — verify unit "removed field bumps major"
#[specforge_test(
    behavior = "compute_schema_version",
    verify = "removed field bumps major"
)]
fn compute_version_removed_field_bumps_major() {
    let migration = SchemaMigration {
        changes: vec![SchemaMigrationChange::FieldRemoved {
            kind: "behavior".to_string(),
            field: "contract".to_string(),
        }],
    };
    let version = compute_schema_version(&migration, Some(&SchemaVersion::new(1, 0, 0)));
    assert_eq!(version, SchemaVersion::new(2, 0, 0));
}

// B:embed_schema_in_export — verify unit "V2 nodes contain all fields"
#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "graph format includes all fields and metadata"
)]
fn a_graph_export_with_a_schema_keeps_every_field() {
    let mut graph = Graph::new();
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("contract"),
        specforge_graph::FieldValue::String("MUST work".to_string()),
    );
    graph.add_node(Node {
        id: EntityId {
            raw: Sym::new("alpha"),
        },
        kind: EntityKind {
            raw: Sym::new("behavior"),
        },
        title: Some("Alpha".to_string()),
        fields,
        source_span: span(),
        methods: Vec::new(),
    });

    let schema = GraphProtocolSchema::empty();
    let json = with_schema(&graph, EmitFormat::Json, None, &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let node = &parsed["nodes"].as_array().unwrap()[0];
    assert_eq!(node["fields"]["contract"], "MUST work");
    assert_eq!(node["file"], "test.spec");
    assert_eq!(node["line"], 1);
}

// B:embed_schema_in_export — verify unit "V2 edges present"
#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "graph format includes all nodes and edges"
)]
fn a_graph_export_with_a_schema_keeps_the_edges() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "behavior", None));
    graph.add_node(node("b", "feature", None));
    graph.add_edge(Edge {
        source: Sym::new("a"),
        target: Sym::new("b"),
        label: Sym::new("implements"),
    });

    let schema = GraphProtocolSchema::empty();
    let json = with_schema(&graph, EmitFormat::Json, None, &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let edges = parsed["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["source"], "a");
    assert_eq!(edges[0]["label"], "implements");
}

// B:negotiate_schema_version — verify unit "above max fails"
#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "incompatible version produces E027 with supported range"
)]
fn negotiate_above_max_fails() {
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    let requested = SchemaVersion::new(1, 6, 0);
    let err = negotiate_version(&requested, &min, &max).unwrap_err();
    assert_eq!(
        err.to_string(),
        "E027: requested schema version 1.6.0 is out of range [1.0.0, 1.5.0]"
    );
}

// B:generate_schema_from_registries — verify unit "edge with no source/target kinds"
#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "schema includes all registered edge types"
)]
fn generate_schema_edge_no_source_target() {
    let build = build_registries(vec![declaration(
        "@specforge/software",
        vec![],
        vec![edge_decl("custom_rel", None, None)],
    )]);

    let schema = generate_schema(&build);
    assert_eq!(schema.edge_types.len(), 1);
    assert!(schema.edge_types[0].source_kinds.is_none());
    assert!(schema.edge_types[0].target_kinds.is_none());
}

// B:detect_breaking_schema_changes — verify unit "multiple field changes tracked"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "SchemaMigration record emitted on version change"
)]
fn diff_multiple_field_changes() {
    let old = sample_schema();
    let mut new = sample_schema();
    // Remove existing field and add a new one
    new.entity_kinds[0].fields.clear();
    new.entity_kinds[0].fields.push(SchemaField {
        name: "description".to_string(),
        field_type: FieldType::String,
        required: false,
        enum_values: None,
        edge: None,
        target_kind: None,
        description: None,
        default_value: None,
        source_extension: "@specforge/software".to_string(),
    });

    let migration = diff_schemas(&old, &new);
    let mut changes = migration.changes.clone();
    changes.sort_by_key(|c| format!("{c:?}"));
    assert_eq!(
        changes,
        vec![
            SchemaMigrationChange::FieldAdded {
                kind: "behavior".to_string(),
                field: "description".to_string(),
                required: false,
            },
            SchemaMigrationChange::FieldRemoved {
                kind: "behavior".to_string(),
                field: "contract".to_string(),
            },
        ]
    );
    assert!(migration.has_breaking_changes()); // removed "contract"
    assert!(migration.has_additions()); // added "description"
}

// B:publish_schema_specification — verify unit "description includes version"
#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "description includes version"
)]
fn publish_json_schema_description_includes_version() {
    let schema = sample_schema();
    let parsed = publish(&schema, EmitFormat::Json).unwrap();
    let desc = parsed["description"].as_str().unwrap();
    assert!(desc.contains("1.2.3"));
}

// ===========================================================================
// Gap coverage: CLI integration (--no-schema, --schema-version)
// ===========================================================================

#[test]
fn schema_version_cli_flag_selects_version() {
    let requested: SchemaVersion = "1.2.0".parse().unwrap();
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    let result = negotiate_version(&requested, &min, &max);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().resolved, SchemaVersion::new(1, 2, 0));
}

#[test]
fn schema_version_mcp_query_parameter() {
    let requested: SchemaVersion = "1.3.0".parse().unwrap();
    let min = SchemaVersion::new(1, 0, 0);
    let max = SchemaVersion::new(1, 5, 0);
    let result = negotiate_version(&requested, &min, &max);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().resolved, SchemaVersion::new(1, 3, 0));
}

// B:detect_breaking_schema_changes — verify unit "SchemaMigration record emitted on version change"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "SchemaMigration record emitted on version change"
)]
fn detect_breaking_migration_record_emitted() {
    let old = GraphProtocolSchema::empty();
    let new = sample_schema();
    let migration = diff_schemas(&old, &new);

    let mut changes = migration.changes.clone();
    changes.sort_by_key(|c| format!("{c:?}"));
    assert_eq!(
        changes,
        vec![
            SchemaMigrationChange::EdgeAdded("implements".to_string()),
            // A new kind's fields come with it: no separate FieldAdded.
            SchemaMigrationChange::KindAdded("behavior".to_string()),
            SchemaMigrationChange::KindAdded("feature".to_string()),
        ]
    );
    // The record serializes for consumers.
    let record = serde_json::to_value(&migration).unwrap();
    assert_eq!(record["changes"].as_array().unwrap().len(), 3);
}

// ===========================================================================
// Gap coverage: Runtime/lifecycle
// ===========================================================================

#[test]
fn schema_generated_deterministically_for_caching() {
    let build = software_and_product();

    let schema1 = generate_schema(&build);
    let schema2 = generate_schema(&build);
    assert_eq!(
        schema1, schema2,
        "Schema generation must be deterministic for caching"
    );
}

// B:serve_schema_resource — verify unit "schema reflects current compilation state"
#[specforge_test(
    behavior = "serve_schema_resource",
    verify = "schema reflects current compilation state"
)]
fn schema_reflects_current_state() {
    // The path `specforge schema` takes: compile the project, build the
    // schema from the compilation's registries, serialize it.
    fn serve(dir: &std::path::Path) -> serde_json::Value {
        let runtime = specforge_component::ComponentRuntime::with_user_cache();
        let ctx = specforge_project::CompiledProject::compile(
            &specforge_common::ProjectRoot::of(dir).unwrap(),
            Some(Arc::new(runtime)),
        );
        let schema = generate_schema(ctx.environment().registries());
        serde_json::to_value(&schema).unwrap()
    }
    fn configure(dir: &std::path::Path, extensions: &[&str]) {
        let config = serde_json::json!({
            "name": "t", "version": "0.1.0", "spec_root": "spec", "extensions": extensions,
        });
        std::fs::write(dir.join("specforge.json"), config.to_string()).unwrap();
    }
    fn names(list: &serde_json::Value, key: &str) -> Vec<String> {
        list.as_array()
            .unwrap()
            .iter()
            .map(|k| k[key].as_str().unwrap().to_string())
            .collect()
    }

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("spec")).unwrap();
    std::fs::write(dir.path().join("spec/main.spec"), "").unwrap();

    configure(dir.path(), &["@specforge/software"]);
    let before = names(&serve(dir.path())["entity_kinds"], "name");
    assert!(before.contains(&"behavior".to_string()), "{before:?}");
    assert!(!before.contains(&"feature".to_string()), "{before:?}");

    // The project now also uses the product extension, which declares
    // `feature`: the next serve reports it.
    configure(dir.path(), &["@specforge/software", "@specforge/product"]);
    let after = serve(dir.path());
    let kinds = names(&after["entity_kinds"], "name");
    assert!(kinds.contains(&"feature".to_string()), "{kinds:?}");
    assert!(kinds.contains(&"behavior".to_string()), "{kinds:?}");
    let extensions = names(&after["extensions"], "name");
    assert!(
        extensions.contains(&"@specforge/product".to_string()),
        "{extensions:?}"
    );
}

// ===========================================================================
// Gap coverage: JSON Schema validation
// ===========================================================================

// Not linked: this checks required keys and the kind enum by hand, not the
// whole export against the schema. schema_publish_validates_a_real_export in
// specforge-cli's e2e_schema tests runs a validator over a real export.
#[test]
fn published_schema_validates_known_good_export() {
    let schema = sample_schema();
    let json_schema = publish(&schema, EmitFormat::Json).unwrap();

    let mut graph = Graph::new();
    graph.add_node(node("alpha", "behavior", Some("Alpha")));
    let export = with_schema(&graph, EmitFormat::Json, None, &schema).unwrap();
    let export_val: serde_json::Value = serde_json::from_str(&export).unwrap();

    assert_eq!(export_val["format_version"], "2.0");
    assert_eq!(
        specforge_common::shape::violations(&json_schema, &export_val),
        Vec::<String>::new()
    );
}

// B:publish_schema_specification — verify unit "published schema describes all registered entity kinds"
#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "published schema describes all registered entity kinds"
)]
fn published_schema_describes_all_kinds() {
    let schema = sample_schema();
    let json_schema = publish(&schema, EmitFormat::Json).unwrap();

    let kinds = published_kinds(&json_schema);
    for ek in &schema.entity_kinds {
        assert!(
            kinds.contains(&ek.name),
            "entity kind '{}' not in published JSON Schema",
            ek.name
        );
    }
}

// B:publish_schema_specification — verify unit "published schema describes all edge types"
#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "published schema describes all edge types"
)]
fn published_schema_describes_all_edge_types() {
    let schema = sample_schema();
    let json_schema = publish(&schema, EmitFormat::Json).unwrap();

    // Registered edge types appear where an export names them: the
    // embedded schema block's edge_types.
    let label_enum = json_schema["$defs"]["GraphProtocolSchema"]["properties"]["edge_types"]
        ["items"]["properties"]["label"]["enum"]
        .as_array()
        .unwrap();
    assert_eq!(label_enum.len(), schema.edge_types.len());
    for et in &schema.edge_types {
        assert!(
            label_enum.contains(&serde_json::Value::String(et.label.clone())),
            "edge type '{}' not in published JSON Schema",
            et.label
        );
    }
}

// ===========================================================================
// Gap coverage: Contract tests (DbC requires/ensures)
// ===========================================================================

// B:generate_schema_from_registries — verify contract "requires/ensures consistency for schema generation from registries"
#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "Generate Schema From Registries: schema generation from registries holds — registries_populated_fired, all_kinds_in_schema, all_edges_in_schema, schema_cached, schema_generated_emitted"
)]
fn generate_schema_contract() {
    let build = software_and_product();

    let schema = generate_schema(&build);

    // ensures: all_kinds_in_schema
    assert_eq!(schema.entity_kinds.len(), build.kinds().len());
    for (_, entry) in build.kinds().iter() {
        assert!(
            schema
                .entity_kinds
                .iter()
                .any(|k| k.name == entry.kind_name)
        );
    }

    // ensures: all_edges_in_schema
    assert_eq!(schema.edge_types.len(), build.edges().len());
    for edge in &schema.edge_types {
        assert!(build.edges().contains(&edge.label));
    }
}

// B:embed_schema_in_export — verify contract "requires/ensures consistency for schema embedding in export"
#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "Embed Schema in Export: schema embedding in export holds — schema_version_computed_fired, validation_complete_fired, schema_embedded, format_version_set, schema_ref_names_full_schema"
)]
fn embed_schema_contract() {
    let schema = sample_schema();
    let mut graph = Graph::new();
    graph.add_node(node("alpha", "behavior", Some("Alpha")));

    let json = with_schema(&graph, EmitFormat::Json, None, &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // ensures: schema_embedded
    assert!(parsed["schema"].is_object());
    // ensures: format_version_set
    assert_eq!(parsed["format_version"], "2.0");
    // ensures: full_project_schema
    assert_eq!(
        parsed["schema"]["entity_kinds"].as_array().unwrap().len(),
        schema.entity_kinds.len()
    );
}

// B:detect_breaking_schema_changes — verify contract "requires/ensures consistency for breaking schema change detection"
#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "Detect Breaking Schema Changes: breaking schema change detection holds — schema_generated_fired, filesystem_available, breaking_changes_classified, nonbreaking_changes_classified, migration_record_emitted, schema_breaking_change_detected_emitted"
)]
fn detect_breaking_contract() {
    let old = sample_schema();

    let mut new_breaking = sample_schema();
    new_breaking.entity_kinds.retain(|k| k.name != "feature");
    let migration = diff_schemas(&old, &new_breaking);
    // ensures: breaking_changes_classified
    assert!(migration.has_breaking_changes());

    let mut new_nonbreaking = sample_schema();
    new_nonbreaking.entity_kinds[0].fields.push(SchemaField {
        name: "notes".to_string(),
        field_type: FieldType::String,
        required: false,
        enum_values: None,
        edge: None,
        target_kind: None,
        description: None,
        default_value: None,
        source_extension: "@specforge/software".to_string(),
    });
    let migration2 = diff_schemas(&old, &new_nonbreaking);
    // ensures: nonbreaking_changes_classified
    assert!(!migration2.has_breaking_changes());
    assert!(migration2.has_additions());

    // ensures: migration_record_emitted
    assert!(!migration.changes.is_empty());
}

// Not linked to the Compute Schema Version contract. `specforge export`
// attaches the computed version (crates/specforge-cli/tests/schema_cache.rs
// proves it through the CLI), but the CLI has no event sink, so the
// schema_breaking_change_detected_fired and schema_version_computed_emitted
// clauses have nothing to assert. This only checks the version arithmetic.
#[test]
fn compute_version_contract() {
    // ensures: first_compilation_baseline
    let empty = SchemaMigration { changes: vec![] };
    let v = compute_schema_version(&empty, None);
    assert_eq!(v, SchemaVersion::new(1, 0, 0));

    // ensures: version_auto_computed — major for breaking
    let breaking = SchemaMigration {
        changes: vec![SchemaMigrationChange::KindRemoved("x".into())],
    };
    let v = compute_schema_version(&breaking, Some(&SchemaVersion::new(1, 2, 3)));
    assert_eq!(v.major, 2);

    // ensures: version_auto_computed — minor for additions
    let addition = SchemaMigration {
        changes: vec![SchemaMigrationChange::KindAdded("y".into())],
    };
    let v = compute_schema_version(&addition, Some(&SchemaVersion::new(1, 2, 3)));
    assert_eq!(v, SchemaVersion::new(1, 3, 0));
}

// B:publish_schema_specification — verify contract "requires/ensures consistency for schema specification publication"
#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "Publish Schema Specification: schema specification publication holds — schema_version_computed_fired, validation_complete_fired, valid_json_schema_produced, all_kinds_described, third_party_usable, render_complete_emitted"
)]
fn publish_schema_contract() {
    let schema = sample_schema();
    let parsed = publish(&schema, EmitFormat::Json).unwrap();

    // ensures: valid_json_schema_produced
    assert_eq!(
        parsed["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    // ensures: all_kinds_described
    let kinds = published_kinds(&parsed);
    assert!(schema.entity_kinds.iter().all(|k| kinds.contains(&k.name)));
    // ensures: third_party_usable
    assert!(parsed["properties"].is_object());
    assert!(parsed["required"].is_array());
}

// ===========================================================================
// Gap coverage: Scoped V2 exports
// ===========================================================================

// B:embed_schema_in_export — verify unit "scoped exports carry schema_ref (url and content_hash) instead of embedded schema"
#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "scoped exports carry schema_ref (url and content_hash) instead of embedded schema"
)]
fn scoped_v2_export_references_schema() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "behavior", Some("A")));
    graph.add_node(node("b", "feature", Some("B")));
    graph.add_edge(Edge {
        source: Sym::new("b"),
        target: Sym::new("a"),
        label: Sym::new("behaviors"),
    });

    let schema = sample_schema();
    let json = with_schema(&graph, EmitFormat::Json, Some("a"), &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["format_version"], "2.0");
    assert!(
        parsed.get("schema").is_none(),
        "scoped export must not embed the full schema: {json}"
    );
    assert_eq!(
        parsed["schema_ref"]["url"],
        "https://specforge.dev/schema/graph-protocol-v1.2.3.json"
    );
    let hash = parsed["schema_ref"]["content_hash"].as_str().unwrap();
    assert_eq!(hash.len(), 64, "sha256 hex hash, got {hash}");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

// B:embed_schema_in_export — verify unit "scoped context V2 export references schema"
#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "scoped exports carry schema_ref (url and content_hash) instead of embedded schema"
)]
fn scoped_context_v2_export() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "behavior", Some("A")));
    let schema = GraphProtocolSchema::empty();
    let json = with_schema(&graph, EmitFormat::Context, Some("a"), &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["format_version"], "2.0");
    assert!(parsed.get("schema").is_none());
    assert!(parsed["schema_ref"]["content_hash"].is_string());
}

// B:embed_schema_in_export — verify unit "scoped brief V2 export references schema"
#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "scoped exports carry schema_ref (url and content_hash) instead of embedded schema"
)]
fn scoped_brief_v2_export() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "behavior", Some("A")));
    let schema = GraphProtocolSchema::empty();
    let json = with_schema(&graph, EmitFormat::Brief, Some("a"), &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["format_version"], "2.0");
    assert!(parsed.get("schema").is_none());
    assert!(parsed["schema_ref"]["content_hash"].is_string());
}

// B:embed_schema_in_export — verify unit "scoped V2 export with nonexistent scope returns error"
#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "non-existent scope entity produces E003 and exit code 1"
)]
fn scoped_v2_nonexistent_scope_error() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "behavior", Some("A")));
    let schema = GraphProtocolSchema::empty();

    let err = with_schema(&graph, EmitFormat::Json, Some("nonexistent"), &schema).unwrap_err();
    assert_eq!(
        err.to_string(),
        "unresolved entity 'nonexistent' — not found in graph"
    );
    assert_eq!(err.code(), Some(specforge_diagnostics::codes::E003));

    // What `specforge export --format graph --scope` calls, with a schema:
    // the same E003.
    let err = specforge_emitter::emit(
        &graph,
        &specforge_emitter::EmitOptions {
            scope: Some("nonexistent"),
            schema: Some(&schema),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "unresolved entity 'nonexistent' — not found in graph"
    );
    assert_eq!(err.code(), Some(specforge_diagnostics::codes::E003));
}

// ===========================================================================
// Gap coverage: compute_schema_version — metadata-only change
// ===========================================================================

#[test]
fn compute_version_metadata_only_no_bump() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.extensions[0].version = "2.0.0".to_string();

    let migration = diff_schemas(&old, &new);
    assert!(migration.is_empty());
    let version = compute_schema_version(&migration, Some(&SchemaVersion::new(1, 2, 3)));
    assert_eq!(
        version,
        SchemaVersion::new(1, 2, 3),
        "no structural change = no version bump"
    );
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "field metadata change triggers patch version bump"
)]
fn field_description_change_bumps_the_patch() {
    let old = sample_schema();
    let mut new = sample_schema();
    new.entity_kinds[0].fields[0].description = Some("reworded".to_string());

    let migration = diff_schemas(&old, &new);

    assert_eq!(migration.changes.len(), 1, "{migration:?}");
    assert!(!migration.has_breaking_changes());
    assert!(!migration.has_additions());
    assert_eq!(
        compute_schema_version(&migration, Some(&SchemaVersion::new(1, 2, 3))),
        SchemaVersion::new(1, 2, 4)
    );
}

// C6-03: SchemaVersion Ord must honor pre-release labels — 1.0.0-beta
// sorts strictly below 1.0.0, and cmp/Eq stay consistent.
#[test]
fn schema_version_orders_prerelease_below_release() {
    use std::cmp::Ordering;

    let beta = "1.0.0-beta".parse::<SchemaVersion>().unwrap();
    let release = "1.0.0".parse::<SchemaVersion>().unwrap();
    let alpha = "1.0.0-alpha".parse::<SchemaVersion>().unwrap();

    assert_eq!(beta.cmp(&release), Ordering::Less);
    assert_eq!(alpha.cmp(&beta), Ordering::Less);
    assert_eq!(release.cmp(&release), Ordering::Equal);
    // Ord/PartialEq consistency: equal ordering implies Eq.
    assert_eq!(
        "1.0.0".parse::<SchemaVersion>().unwrap() == "1.0.0".parse::<SchemaVersion>().unwrap(),
        "1.0.0"
            .parse::<SchemaVersion>()
            .unwrap()
            .cmp(&"1.0.0".parse::<SchemaVersion>().unwrap())
            == Ordering::Equal
    );
}

// C6-07: full exports keep embedding the schema; only scoped exports
// downgrade to a schema_ref.
#[test]
fn full_v2_export_still_embeds_schema() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "behavior", Some("A")));
    let schema = sample_schema();

    let json = with_schema(&graph, EmitFormat::Brief, None, &schema).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed["schema"]["entity_kinds"].as_array().unwrap().len() == 2,
        "full export embeds the whole schema"
    );
    assert!(
        parsed.get("schema_ref").is_none(),
        "full export must not carry a schema_ref"
    );
}

// C6-04: the published schema constrains field names and types per kind,
// forbids unknown properties, and describes schema_ref.
#[test]
fn published_schema_constrains_fields_per_kind() {
    let schema = sample_schema();

    let parsed = publish(&schema, EmitFormat::Json).unwrap();

    assert_eq!(parsed["additionalProperties"], false, "root closed");
    assert!(
        parsed["$defs"]["GraphProtocolSchema"].is_object(),
        "the embedded schema's shape is defined"
    );
    let behavior_fields = &parsed["$defs"]["fields_behavior"];
    assert_eq!(behavior_fields["type"], "object");
    assert_eq!(
        behavior_fields["properties"]["contract"],
        serde_json::json!({ "type": "string" }),
        "behavior.contract typed from the registry"
    );
    assert!(
        parsed["$defs"].get("fields_feature").is_some(),
        "per-kind def exists even when the kind declares no fields"
    );
    // Any field a kind does not declare is still a field value.
    assert_eq!(
        behavior_fields["additionalProperties"],
        serde_json::json!({ "$ref": "#/$defs/FieldJson" })
    );

    // Node items are one branch per kind; the branch names its kind and
    // the kind's fields.
    let branches = parsed["properties"]["nodes"]["items"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(branches.len(), schema.entity_kinds.len() + 2);
    let behavior = &parsed["$defs"]["node_behavior"];
    assert_eq!(
        behavior["properties"]["kind"],
        serde_json::json!({ "type": "string", "enum": ["behavior"] })
    );
    assert_eq!(
        behavior["properties"]["fields"]["$ref"],
        "#/$defs/fields_behavior"
    );

    // Brief nodes have no fields, so no per-kind defs or branches there.
    let brief = publish(&schema, EmitFormat::Brief).unwrap();
    assert!(brief["$defs"].get("fields_behavior").is_none());
    assert!(brief["properties"]["nodes"]["items"].get("oneOf").is_none());

    // schema_ref shape is published for scoped consumers.
    assert_eq!(
        parsed["$defs"]["SchemaRefBlock"]["required"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

// B:publish_schema_specification — verify unit "published context schema admits declared headline and normative fields"
#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "published context schema admits declared headline and normative fields"
)]
fn published_context_schema_admits_declared_headline_and_normative_fields() {
    let mut fields = FieldRegistry::new();
    // A headline field named neither `contract` nor `status`.
    let stage = amended(
        make_field_entry("task", "stage", FieldType::String, false),
        |d| d.headline = true,
    );
    fields.register(stage);
    let goal = amended(
        make_field_entry("task", "goal", FieldType::String, false),
        |d| d.normative = true,
    );
    fields.register(goal);

    let mut task = node("ship", "task", Some("Ship it"));
    task.fields
        .push(Sym::new("stage"), FieldValue::String("doing".to_string()));
    task.fields.push(
        Sym::new("goal"),
        FieldValue::String("users have it".to_string()),
    );
    let mut graph = Graph::new();
    graph.add_node(task);

    let schema = GraphProtocolSchema::empty();
    let export = specforge_emitter::emit(
        &graph,
        &specforge_emitter::EmitOptions {
            format: EmitFormat::Context,
            schema: Some(&schema),
            field_registry: Some(&fields),
            ..Default::default()
        },
    )
    .unwrap();
    let export: serde_json::Value = serde_json::from_str(&export).unwrap();
    let published = publish(&schema, EmitFormat::Context).unwrap();
    let node_schema = &published["properties"]["nodes"]["items"];
    let properties = node_schema["properties"].as_object().unwrap();
    assert!(!properties.contains_key("contract"), "{node_schema}");
    assert!(!properties.contains_key("status"), "{node_schema}");
    assert_eq!(properties["fields"]["type"], "object");
    assert_eq!(
        node_schema["additionalProperties"],
        serde_json::json!({ "type": "string" })
    );

    let node = &export["nodes"][0];
    assert_eq!(node["stage"], "doing");
    assert_eq!(node["fields"]["goal"], "users have it");
    for (key, value) in node.as_object().unwrap() {
        assert!(
            properties.contains_key(key) || value.is_string(),
            "context node key '{key}' is neither declared nor a string: {value}"
        );
    }
}

/// `urgent` (bool), `priority` (enum low/high) and `owner` (string,
/// default "nobody"), declared by `@t/x` on `ticket` and read through the
/// registry build.
fn ticket_registries() -> specforge_registry::RegistryBuild {
    use specforge_protocol_types::{EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor};
    let field = |name: &str, field_type: &str| FieldDescriptor {
        name: name.to_string(),
        field_type: field_type.to_string(),
        ..Default::default()
    };
    let mut declaration = ExtensionDeclaration::default();
    declaration.handshake.name = "@t/x".to_string();
    declaration.entities = vec![EntityKindDescriptor {
        name: "ticket".to_string(),
        keyword: Some("ticket".to_string()),
        fields: vec![
            field("urgent", "bool"),
            FieldDescriptor {
                enum_values: vec!["low".to_string(), "high".to_string()],
                ..field("priority", "enum")
            },
            FieldDescriptor {
                default_value: Some("nobody".to_string()),
                ..field("owner", "string")
            },
        ],
        ..Default::default()
    }];
    specforge_registry::build_registries(vec![declaration])
}

#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "a field's type is named in the schema as its extension declares it"
)]
fn a_field_type_is_named_in_the_schema_as_declared() {
    let build = ticket_registries();
    let schema = generate_schema(&build);
    let ticket = schema
        .entity_kinds
        .iter()
        .find(|k| k.name == "ticket")
        .unwrap();
    let field = |name: &str| ticket.fields.iter().find(|f| f.name == name).unwrap();

    assert_eq!(field("urgent").field_type, FieldType::Bool);
    assert_eq!(
        field("priority").enum_values,
        Some(vec!["low".to_string(), "high".to_string()])
    );
    let written = serde_json::to_string(&schema).unwrap();
    assert!(written.contains(r#""field_type":"bool""#), "{written}");
    assert!(!written.contains("boolean"), "{written}");
}

#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "the published schema lists the field types the host reads"
)]
fn the_published_schema_lists_the_field_types_the_host_reads() {
    let build = ticket_registries();
    let schema = generate_schema(&build);
    let published = publish(&schema, EmitFormat::Json).unwrap();
    let listed = &published["$defs"]["GraphProtocolSchema"]["properties"]["entity_kinds"]["items"]
        ["properties"]["fields"]["items"]["properties"]["field_type"]["enum"];
    let names: Vec<&str> = FieldType::ALL.iter().map(|t| t.as_str()).collect();
    assert_eq!(
        names,
        [
            "string",
            "integer",
            "bool",
            "enum",
            "string_list",
            "reference",
            "reference_list",
            "block"
        ]
    );
    assert_eq!(listed, &serde_json::json!(names));
}

#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "a field's declared default value reaches the schema"
)]
fn the_schema_carries_a_declared_default_value() {
    let build = ticket_registries();
    let schema = generate_schema(&build);
    let ticket = schema
        .entity_kinds
        .iter()
        .find(|k| k.name == "ticket")
        .unwrap();
    let field = |name: &str| ticket.fields.iter().find(|f| f.name == name).unwrap();
    assert_eq!(field("owner").default_value.as_deref(), Some("nobody"));
    assert_eq!(field("urgent").default_value, None);

    // The model built from the schema shows it.
    let model = specforge_emitter::model::export(
        &schema,
        &[],
        &specforge_emitter::model::ModelOptions {
            format: specforge_emitter::model::ModelFormat::Json,
            fields: specforge_emitter::model::FieldLevel::All,
            ..Default::default()
        },
    );
    let model: serde_json::Value = serde_json::from_str(&model).unwrap();
    let owner = model["entities"][0]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "owner")
        .expect("owner is a field of the model");
    assert_eq!(owner["default_value"], "nobody");
}

// B:generate_schema_from_registries — an edge type that leaves a side open takes its writers
#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "an edge type that declares no source or target kind names those of the fields that write it"
)]
fn an_open_edge_type_takes_the_kinds_of_the_fields_writing_it() {
    let build = build_registries(vec![declaration(
        "@specforge/software",
        vec![
            kind_decl(
                "a",
                false,
                vec![
                    edge_writer_decl("x", "relates", "c"),
                    edge_writer_decl("z", "owns", "c"),
                ],
            ),
            kind_decl(
                "b",
                false,
                vec![
                    edge_writer_decl("y", "relates", "a"),
                    edge_writer_decl("w", "owns", "b"),
                ],
            ),
            kind_decl("c", false, vec![]),
        ],
        vec![
            edge_decl("relates", None, None),
            edge_decl("owns", Some("a"), None),
        ],
    )]);

    let schema = generate_schema(&build);
    let edge = |label: &str| {
        schema
            .edge_types
            .iter()
            .find(|e| e.label == label)
            .unwrap()
            .clone()
    };
    let relates = edge("relates");
    assert_eq!(relates.source_kinds, Some(vec!["a".into(), "b".into()]));
    assert_eq!(relates.target_kinds, Some(vec!["a".into(), "c".into()]));
    // A side the edge type declares is kept; the open side is the writers',
    // sorted. `b.w` writes `owns` from outside its declared source `a`: it
    // is W021 and writes no `owns` edge, so `b` is no target of it.
    let owns = edge("owns");
    assert_eq!(owns.source_kinds, Some(vec!["a".into()]));
    assert_eq!(owns.target_kinds, Some(vec!["c".into()]));
    let w021: Vec<&str> = build
        .diagnostics()
        .filter(|d| d.code == "W021")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        w021,
        [
            "extension '@specforge/software': field 'w' on kind 'b' writes edge type 'owns', whose declared source_kind is 'a' — its edges are not of that type"
        ]
    );
}

#[specforge_test(
    behavior = "generate_schema_from_registries",
    verify = "an edge type several kinds write without declaring it names every writing kind"
)]
fn an_undeclared_edge_two_kinds_write_names_every_writer() {
    use specforge_protocol_types::{EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor};
    let writer = |name: &str| EntityKindDescriptor {
        name: name.to_string(),
        fields: vec![FieldDescriptor {
            name: "to_gamma".to_string(),
            field_type: FieldType::ReferenceList.as_str().to_string(),
            edge: Some("links".to_string()),
            target_kind: Some("gamma".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };
    let declaration = ExtensionDeclaration {
        entities: vec![
            writer("alpha"),
            writer("beta"),
            EntityKindDescriptor {
                name: "gamma".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let build = specforge_registry::build_registries(vec![declaration]);

    let schema = generate_schema(&build);
    let links = schema
        .edge_types
        .iter()
        .find(|e| e.label == "links")
        .expect("the implicit edge type is registered");
    assert_eq!(
        links.source_kinds,
        Some(vec!["alpha".to_string(), "beta".to_string()])
    );
    assert_eq!(links.target_kinds, Some(vec!["gamma".to_string()]));
}

// ===========================================================================
// The published schema is the export document's, narrowed by the project
// (ADR-NEW-01)
// ===========================================================================

const AGENT_FORMATS: [EmitFormat; 3] = [EmitFormat::Json, EmitFormat::Context, EmitFormat::Brief];

#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "the published schema is the export document's schema narrowed by the project"
)]
fn the_published_schema_is_the_export_documents_narrowed() {
    let schema = sample_schema();
    for format in AGENT_FORMATS {
        let mut defs = specforge_common::shape::Definitions::default();
        let derived = specforge_emitter::document_schema(format, &mut defs).unwrap();
        let derived = defs.finish(derived);
        let published = publish(&schema, format).unwrap();

        // Every named shape of the export document is in the published one.
        for name in derived["$defs"].as_object().unwrap().keys() {
            assert!(
                published["$defs"].get(name).is_some(),
                "{format:?}: {name} is not defined"
            );
        }
        // The root says what the export document says.
        let keys = |document: &serde_json::Value| -> Vec<String> {
            document["properties"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect()
        };
        assert_eq!(keys(&published), keys(&derived), "{format:?}");
    }

    // Each declared field's schema is its field form's.
    let published = publish(&schema, EmitFormat::Json).unwrap();
    let mut defs = specforge_common::shape::Definitions::default();
    for kind in &schema.entity_kinds {
        for field in &kind.fields {
            assert_eq!(
                published["$defs"][format!("fields_{}", kind.name)]["properties"][&field.name],
                field.form().json_schema(&mut defs),
                "{}.{}",
                kind.name,
                field.name
            );
        }
    }
}

#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "the published schema uses only keywords the in-repo checker reads"
)]
fn the_published_schema_uses_only_keywords_the_checker_reads() {
    for schema in [sample_schema(), GraphProtocolSchema::empty()] {
        for format in AGENT_FORMATS {
            let published = publish(&schema, format).unwrap();
            assert_eq!(
                specforge_common::shape::unchecked_keywords(&published),
                Vec::<String>::new(),
                "{format:?}"
            );
        }
    }
}

#[test]
fn each_format_publishes_its_own_id() {
    let schema = sample_schema();
    let ids: Vec<String> = AGENT_FORMATS
        .iter()
        .map(|format| {
            publish(&schema, *format).unwrap()["$id"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        ids,
        [
            "https://specforge.dev/schema/graph-protocol-v1.2.3.json",
            "https://specforge.dev/schema/graph-protocol-v1.2.3-context.json",
            "https://specforge.dev/schema/graph-protocol-v1.2.3-brief.json",
        ]
    );
    assert!(publish(&schema, EmitFormat::Dot).is_none());
}
