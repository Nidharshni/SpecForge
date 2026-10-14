use insta::assert_snapshot;
use serde_json::Value;
use specforge_emitter::model::{FieldLevel, GroupBy, ModelFormat, ModelOptions, ModelRoot, export};
use specforge_emitter::schema::*;
use specforge_protocol_types::{ExtensionDeclaration, HandshakeResponse};
use specforge_registry::FieldType;

/// The model of `schema` as `options` asks, with no declarations (every
/// extension grey).
fn exported(schema: &GraphProtocolSchema, options: ModelOptions) -> String {
    export(schema, &[], &options)
}

/// The JSON model `options` selects over `schema`.
fn json_of(schema: &GraphProtocolSchema, options: ModelOptions) -> Value {
    serde_json::from_str(&exported(
        schema,
        ModelOptions {
            format: ModelFormat::Json,
            ..options
        },
    ))
    .expect("the json model parses")
}

/// The whole model of `schema` with every field: its intermediate
/// representation, as the JSON format serializes it.
fn built(schema: &GraphProtocolSchema) -> Value {
    json_of(
        schema,
        ModelOptions {
            fields: FieldLevel::All,
            ..ModelOptions::default()
        },
    )
}

fn names(list: &Value) -> Vec<&str> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect()
}

/// The declarations giving the builtins' `theme_color`s.
fn builtin_colors() -> Vec<ExtensionDeclaration> {
    [
        ("@specforge/software", "#4a90d9"),
        ("@specforge/product", "#2ecc71"),
        ("@specforge/governance", "#e74c3c"),
        ("@specforge/formal", "#9b59b6"),
    ]
    .into_iter()
    .map(|(name, color)| ExtensionDeclaration {
        handshake: HandshakeResponse {
            name: name.into(),
            theme_color: Some(color.into()),
            ..HandshakeResponse::default()
        },
        ..ExtensionDeclaration::default()
    })
    .collect()
}

// =========================================================================
// Tracer bullet: types + Display impls
// =========================================================================

#[test]
fn model_options_defaults() {
    let opts = ModelOptions::default();
    assert_eq!(opts.format, ModelFormat::Markdown);
    assert_eq!(opts.group_by, GroupBy::Extension);
    assert_eq!(opts.fields, FieldLevel::Keys);
    assert!(opts.extension.is_none());
    assert!(opts.kinds.is_empty());
    assert!(opts.root.is_none());
}

// =========================================================================
// Builder: empty schema -> empty IR
// =========================================================================

#[test]
fn empty_schema_produces_empty_model() {
    let schema = GraphProtocolSchema::empty();
    let model = built(&schema);

    assert_eq!(model["schema_version"], "1.0.0");
    for list in ["extensions", "entities", "relationships"] {
        assert!(model[list].as_array().unwrap().is_empty(), "{list}");
    }
}

// =========================================================================
// Builder: single entity kind -> one ModelEntity with synthetic id field
// =========================================================================

#[test]
fn single_entity_kind_maps_to_model_entity_with_id() {
    let schema = GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
        extensions: vec![SchemaExtensionInfo {
            name: "@specforge/software".to_string(),
            version: "1.0.0".to_string(),
        }],
        entity_kinds: vec![SchemaEntityKind {
            name: "behavior".to_string(),
            source_extension: "@specforge/software".to_string(),
            testable: true,
            dot_color: None,
            fields: vec![],
        }],
        edge_types: vec![],
    };

    let model = built(&schema);

    assert_eq!(model["entities"].as_array().unwrap().len(), 1);
    let entity = &model["entities"][0];
    assert_eq!(entity["name"], "behavior");
    assert_eq!(entity["extension"], "@specforge/software");

    // Synthetic id field should be first
    assert!(!entity["fields"].as_array().unwrap().is_empty());
    let id_field = &entity["fields"][0];
    assert_eq!(id_field["name"], "id");
    assert_eq!(id_field["field_type"], "string");
    assert_eq!(id_field["required"], true);
    assert_eq!(id_field["is_primary_key"], true);
}

// =========================================================================
// Builder: entity with schema fields -> correct field mapping
// =========================================================================

#[test]
fn entity_fields_mapped_from_schema() {
    let schema = GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
        extensions: vec![SchemaExtensionInfo {
            name: "@specforge/software".to_string(),
            version: "1.0.0".to_string(),
        }],
        entity_kinds: vec![SchemaEntityKind {
            name: "behavior".to_string(),
            source_extension: "@specforge/software".to_string(),
            testable: true,
            dot_color: None,
            fields: vec![
                SchemaField {
                    name: "status".to_string(),
                    field_type: FieldType::Enum,
                    required: true,
                    enum_values: Some(vec!["draft".to_string(), "approved".to_string()]),
                    edge: None,
                    target_kind: None,
                    description: Some("Current status".to_string()),
                    default_value: None,
                    source_extension: "@specforge/software".to_string(),
                },
                SchemaField {
                    name: "features".to_string(),
                    field_type: FieldType::ReferenceList,
                    required: false,
                    enum_values: None,
                    edge: Some("BehaviorImplementsFeature".to_string()),
                    target_kind: Some("feature".to_string()),
                    description: None,
                    default_value: None,
                    source_extension: "@specforge/software".to_string(),
                },
            ],
        }],
        edge_types: vec![],
    };

    let model = built(&schema);

    let entity = &model["entities"][0];
    // id + 2 schema fields = 3 fields
    assert_eq!(entity["fields"].as_array().unwrap().len(), 3);

    // First field is always the synthetic id
    assert_eq!(entity["fields"][0]["name"], "id");
    assert_eq!(entity["fields"][0]["is_primary_key"], true);

    // Status field
    let status = &entity["fields"][1];
    assert_eq!(status["name"], "status");
    assert_eq!(status["field_type"], "enum");
    assert_eq!(status["required"], true);
    assert_eq!(
        status["enum_values"],
        serde_json::json!(["draft", "approved"])
    );
    assert_eq!(status["description"], "Current status");
    assert_eq!(status["is_primary_key"], false);

    // Features field (reference_list -> has references)
    let features = &entity["fields"][2];
    assert_eq!(features["name"], "features");
    assert_eq!(features["field_type"], "reference_list");
    assert_eq!(features["required"], false);
    assert_eq!(features["references"], "feature");
}

// =========================================================================
// Builder: edge types -> ModelRelationship with cardinality
// =========================================================================

#[test]
fn edge_type_maps_to_relationship() {
    let schema = GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
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
                    name: "features".to_string(),
                    field_type: FieldType::ReferenceList,
                    required: false,
                    enum_values: None,
                    edge: Some("BehaviorImplementsFeature".to_string()),
                    target_kind: Some("feature".to_string()),
                    description: None,
                    default_value: None,
                    source_extension: "@specforge/software".to_string(),
                }],
            },
            SchemaEntityKind {
                name: "feature".to_string(),
                source_extension: "@specforge/software".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
        ],
        edge_types: vec![SchemaEdgeType {
            label: "BehaviorImplementsFeature".to_string(),
            source_extension: "@specforge/software".to_string(),
            source_kinds: Some(vec!["behavior".to_string()]),
            target_kinds: Some(vec!["feature".to_string()]),
        }],
    };

    let model = built(&schema);

    assert_eq!(model["relationships"].as_array().unwrap().len(), 1);
    let rel = &model["relationships"][0];
    assert_eq!(rel["name"], "BehaviorImplementsFeature");
    assert_eq!(rel["source"], "behavior");
    assert_eq!(rel["target"], "feature");
    // reference_list field -> ManyToMany
    assert_eq!(rel["cardinality"], "N:M");
    assert_eq!(rel["source_field"], "features");
}

// =========================================================================
// Builder: extension metadata counts
// =========================================================================

#[test]
fn extension_metadata_counts() {
    let schema = GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
        extensions: vec![
            SchemaExtensionInfo {
                name: "@specforge/software".to_string(),
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
                source_extension: "@specforge/software".to_string(),
                testable: true,
                dot_color: None,
                fields: vec![],
            },
            SchemaEntityKind {
                name: "event".to_string(),
                source_extension: "@specforge/software".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
            SchemaEntityKind {
                name: "feature".to_string(),
                source_extension: "@specforge/product".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
        ],
        edge_types: vec![
            SchemaEdgeType {
                label: "Triggers".to_string(),
                source_extension: "@specforge/software".to_string(),
                source_kinds: Some(vec!["behavior".to_string()]),
                target_kinds: Some(vec!["event".to_string()]),
            },
            SchemaEdgeType {
                label: "BehaviorImplementsFeature".to_string(),
                source_extension: "@specforge/software".to_string(),
                source_kinds: Some(vec!["behavior".to_string()]),
                target_kinds: Some(vec!["feature".to_string()]),
            },
        ],
    };

    let model = built(&schema);

    assert_eq!(model["extensions"].as_array().unwrap().len(), 2);

    let extension = |name: &str| {
        model["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == name)
            .unwrap()
            .clone()
    };
    let sw = extension("@specforge/software");
    assert_eq!(sw["entity_count"], 2);
    assert_eq!(sw["edge_count"], 2);

    let prod = extension("@specforge/product");
    assert_eq!(prod["entity_count"], 1);
    assert_eq!(prod["edge_count"], 0);
}

// =========================================================================
// Cardinality: reference field -> ManyToOne
// =========================================================================

#[test]
fn reference_field_infers_many_to_one() {
    let schema = GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
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
                    name: "parent".to_string(),
                    field_type: FieldType::Reference,
                    required: false,
                    enum_values: None,
                    edge: Some("BelongsTo".to_string()),
                    target_kind: Some("feature".to_string()),
                    description: None,
                    default_value: None,
                    source_extension: "@specforge/software".to_string(),
                }],
            },
            SchemaEntityKind {
                name: "feature".to_string(),
                source_extension: "@specforge/software".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
        ],
        edge_types: vec![SchemaEdgeType {
            label: "BelongsTo".to_string(),
            source_extension: "@specforge/software".to_string(),
            source_kinds: Some(vec!["behavior".to_string()]),
            target_kinds: Some(vec!["feature".to_string()]),
        }],
    };

    let model = built(&schema);

    assert_eq!(model["relationships"].as_array().unwrap().len(), 1);
    assert_eq!(model["relationships"][0]["cardinality"], "N:1");
    assert_eq!(model["relationships"][0]["source_field"], "parent");
}

// =========================================================================
// Cardinality: no matching field -> ManyToMany default
// =========================================================================

#[test]
fn no_matching_field_defaults_to_many_to_many() {
    let schema = GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
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
                fields: vec![], // no fields at all
            },
            SchemaEntityKind {
                name: "event".to_string(),
                source_extension: "@specforge/software".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
        ],
        edge_types: vec![SchemaEdgeType {
            label: "Triggers".to_string(),
            source_extension: "@specforge/software".to_string(),
            source_kinds: Some(vec!["behavior".to_string()]),
            target_kinds: Some(vec!["event".to_string()]),
        }],
    };

    let model = built(&schema);

    assert_eq!(model["relationships"].as_array().unwrap().len(), 1);
    assert_eq!(model["relationships"][0]["cardinality"], "N:M");
    assert!(model["relationships"][0].get("source_field").is_none());
}

// =========================================================================
// Cardinality: edge with no source_kinds -> skipped
// =========================================================================

#[test]
fn edge_with_no_source_kinds_skipped() {
    let schema = GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
        extensions: vec![],
        entity_kinds: vec![],
        edge_types: vec![SchemaEdgeType {
            label: "Phantom".to_string(),
            source_extension: "@specforge/software".to_string(),
            source_kinds: None,
            target_kinds: Some(vec!["feature".to_string()]),
        }],
    };

    let model = built(&schema);
    assert!(model["relationships"].as_array().unwrap().is_empty());
}

// =========================================================================
// Cardinality: reference (singular) field -> ManyToOne (term.module -> module)
// =========================================================================

#[test]
fn reference_singular_field_infers_many_to_one_for_term_module() {
    let schema = GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
        extensions: vec![SchemaExtensionInfo {
            name: "@specforge/product".to_string(),
            version: "1.0.0".to_string(),
        }],
        entity_kinds: vec![
            SchemaEntityKind {
                name: "term".to_string(),
                source_extension: "@specforge/product".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![SchemaField {
                    name: "module".to_string(),
                    field_type: FieldType::Reference,
                    required: false,
                    enum_values: None,
                    edge: Some("TermBelongsToModule".to_string()),
                    target_kind: Some("module".to_string()),
                    description: Some("The module that owns this term".to_string()),
                    default_value: None,
                    source_extension: "@specforge/product".to_string(),
                }],
            },
            SchemaEntityKind {
                name: "module".to_string(),
                source_extension: "@specforge/product".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
        ],
        edge_types: vec![SchemaEdgeType {
            label: "TermBelongsToModule".to_string(),
            source_extension: "@specforge/product".to_string(),
            source_kinds: Some(vec!["term".to_string()]),
            target_kinds: Some(vec!["module".to_string()]),
        }],
    };

    let model = built(&schema);

    assert_eq!(model["relationships"].as_array().unwrap().len(), 1);
    let rel = &model["relationships"][0];
    assert_eq!(rel["name"], "TermBelongsToModule");
    assert_eq!(rel["source"], "term");
    assert_eq!(rel["target"], "module");
    assert_eq!(rel["cardinality"], "N:1");
    assert_eq!(rel["source_field"], "module");

    // term entity should have contribution info on the module field
    let term = model["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "term")
        .unwrap();
    let module_field = term["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "module")
        .unwrap();
    assert_eq!(
        module_field["contribution"],
        "TermBelongsToModule -> module"
    );
}

// =========================================================================
// Helper: build a multi-extension model for filter tests
// =========================================================================

fn multi_extension_schema() -> GraphProtocolSchema {
    GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
        extensions: vec![
            SchemaExtensionInfo {
                name: "@specforge/software".to_string(),
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
                source_extension: "@specforge/software".to_string(),
                testable: true,
                dot_color: None,
                fields: vec![
                    SchemaField {
                        name: "contract".to_string(),
                        field_type: FieldType::String,
                        required: true,
                        enum_values: None,
                        edge: None,
                        target_kind: None,
                        description: Some("The contract".to_string()),
                        default_value: None,
                        source_extension: "@specforge/software".to_string(),
                    },
                    SchemaField {
                        name: "features".to_string(),
                        field_type: FieldType::ReferenceList,
                        required: false,
                        enum_values: None,
                        edge: Some("BehaviorImplementsFeature".to_string()),
                        target_kind: Some("feature".to_string()),
                        description: None,
                        default_value: None,
                        source_extension: "@specforge/software".to_string(),
                    },
                ],
            },
            SchemaEntityKind {
                name: "event".to_string(),
                source_extension: "@specforge/software".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
            SchemaEntityKind {
                name: "feature".to_string(),
                source_extension: "@specforge/product".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![SchemaField {
                    name: "priority".to_string(),
                    field_type: FieldType::Enum,
                    required: false,
                    enum_values: Some(vec!["low".into(), "medium".into(), "high".into()]),
                    edge: None,
                    target_kind: None,
                    description: None,
                    default_value: None,
                    source_extension: "@specforge/product".to_string(),
                }],
            },
            SchemaEntityKind {
                name: "journey".to_string(),
                source_extension: "@specforge/product".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
        ],
        edge_types: vec![
            SchemaEdgeType {
                label: "BehaviorImplementsFeature".to_string(),
                source_extension: "@specforge/software".to_string(),
                source_kinds: Some(vec!["behavior".to_string()]),
                target_kinds: Some(vec!["feature".to_string()]),
            },
            SchemaEdgeType {
                label: "Triggers".to_string(),
                source_extension: "@specforge/software".to_string(),
                source_kinds: Some(vec!["behavior".to_string()]),
                target_kinds: Some(vec!["event".to_string()]),
            },
            SchemaEdgeType {
                label: "JourneyExercisesFeature".to_string(),
                source_extension: "@specforge/product".to_string(),
                source_kinds: Some(vec!["journey".to_string()]),
                target_kinds: Some(vec!["feature".to_string()]),
            },
        ],
    }
}

// =========================================================================
// Filter: extension filter keeps only matching entities
// =========================================================================

#[specforge_test_macros::test(
    behavior = "filter_model",
    verify = "extension filter includes only matching entities"
)]
fn filter_by_extension() {
    let m = json_of(
        &multi_extension_schema(),
        ModelOptions {
            fields: FieldLevel::All,
            extension: Some("@specforge/software".to_string()),
            ..ModelOptions::default()
        },
    );
    assert_eq!(names(&m["entities"]), vec!["behavior", "event"]);
    // Implements goes behavior->feature, but feature is filtered out, so pruned
    // Triggers stays (behavior->event, both in software)
    assert_eq!(names(&m["relationships"]), vec!["Triggers"]);
}

// =========================================================================
// Filter: kind filter with known kinds
// =========================================================================

#[specforge_test_macros::test(
    behavior = "filter_model",
    verify = "kind filter includes only listed kinds"
)]
fn filter_by_kinds() {
    let m = json_of(
        &multi_extension_schema(),
        ModelOptions {
            fields: FieldLevel::All,
            kinds: vec!["behavior".to_string(), "feature".to_string()],
            ..ModelOptions::default()
        },
    );
    assert_eq!(names(&m["entities"]), vec!["behavior", "feature"]);
    // Implements stays (behavior->feature), Triggers pruned (event not in filter)
    assert_eq!(
        names(&m["relationships"]),
        vec!["BehaviorImplementsFeature"]
    );
}

// =========================================================================
// Filter: unknown kind silently ignored
// =========================================================================

#[specforge_test_macros::test(
    behavior = "filter_model",
    verify = "a listed kind the project does not know selects nothing in the export"
)]
fn filter_unknown_kind_ignored() {
    let m = json_of(
        &multi_extension_schema(),
        ModelOptions {
            fields: FieldLevel::All,
            kinds: vec!["nonexistent".to_string()],
            ..ModelOptions::default()
        },
    );
    assert!(names(&m["entities"]).is_empty());
    assert!(names(&m["relationships"]).is_empty());
}

// =========================================================================
// Filter: root+depth=0 keeps only root
// =========================================================================

#[specforge_test_macros::test(
    behavior = "filter_model",
    verify = "root+depth=0 includes only the root kind"
)]
fn filter_root_depth_zero() {
    let m = json_of(
        &multi_extension_schema(),
        ModelOptions {
            fields: FieldLevel::All,
            root: Some(ModelRoot {
                kind: "behavior".to_string(),
                depth: Some(0),
            }),
            ..ModelOptions::default()
        },
    );
    assert_eq!(names(&m["entities"]), vec!["behavior"]);
    assert!(names(&m["relationships"]).is_empty());
}

// =========================================================================
// Filter: root+depth=1 keeps root + direct neighbors
// =========================================================================

#[specforge_test_macros::test(
    behavior = "filter_model",
    verify = "root+depth=1 includes root and directly connected kinds"
)]
fn filter_root_depth_one() {
    let m = json_of(
        &multi_extension_schema(),
        ModelOptions {
            fields: FieldLevel::All,
            root: Some(ModelRoot {
                kind: "behavior".to_string(),
                depth: Some(1),
            }),
            ..ModelOptions::default()
        },
    );
    let mut kinds = names(&m["entities"]);
    kinds.sort();
    // behavior is root, direct neighbors via edges: event (Triggers), feature (Implements)
    assert_eq!(kinds, vec!["behavior", "event", "feature"]);
}

// =========================================================================
// Filter: intersection of extension + kind filter
// =========================================================================

#[specforge_test_macros::test(
    behavior = "filter_model",
    verify = "multiple filters compose as intersection"
)]
fn filter_intersection_extension_and_kind() {
    let m = json_of(
        &multi_extension_schema(),
        ModelOptions {
            fields: FieldLevel::All,
            extension: Some("@specforge/software".to_string()),
            kinds: vec!["behavior".to_string()],
            ..ModelOptions::default()
        },
    );
    assert_eq!(names(&m["entities"]), vec!["behavior"]);
}

// =========================================================================
// Filter: field level none -> empty fields
// =========================================================================

#[test]
fn filter_fields_none() {
    let m = json_of(
        &multi_extension_schema(),
        ModelOptions {
            fields: FieldLevel::None,
            ..ModelOptions::default()
        },
    );

    for entity in m["entities"].as_array().unwrap() {
        assert!(
            entity["fields"].as_array().unwrap().is_empty(),
            "entity {} should have no fields",
            entity["name"]
        );
    }
}

// =========================================================================
// Filter: field level keys -> pk + required + refs
// =========================================================================

#[test]
fn filter_fields_keys() {
    let m = json_of(
        &multi_extension_schema(),
        ModelOptions {
            fields: FieldLevel::Keys,
            ..ModelOptions::default()
        },
    );
    let fields_of = |kind: &str| -> Vec<String> {
        let entity = m["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == kind)
            .unwrap();
        names(&entity["fields"])
            .into_iter()
            .map(String::from)
            .collect()
    };

    // id (pk) + contract (required) + features (reference_list)
    assert_eq!(fields_of("behavior"), vec!["id", "contract", "features"]);
    // id (pk) only — priority is not required and not a reference
    assert_eq!(fields_of("feature"), vec!["id"]);
}

// =========================================================================
// Filter: field level all -> all fields unchanged
// =========================================================================

#[test]
fn filter_fields_all() {
    let m = built(&multi_extension_schema());
    let count = |kind: &str| {
        m["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == kind)
            .unwrap()["fields"]
            .as_array()
            .unwrap()
            .len()
    };

    assert_eq!(count("behavior"), 3); // id + contract + features
    assert_eq!(count("feature"), 2); // id + priority
}

// =========================================================================
// Renderer tests — use multi_extension_schema for all snapshots
// =========================================================================

fn default_options(format: ModelFormat) -> ModelOptions {
    ModelOptions {
        format,
        ..ModelOptions::default()
    }
}

// --- Markdown ---

#[test]
fn render_markdown_keys_grouped() {
    let output = exported(
        &multi_extension_schema(),
        default_options(ModelFormat::Markdown),
    );
    assert_snapshot!(output);
}

#[test]
fn render_markdown_none_fields() {
    let output = exported(
        &multi_extension_schema(),
        ModelOptions {
            fields: FieldLevel::None,
            ..default_options(ModelFormat::Markdown)
        },
    );
    assert_snapshot!(output);
}

#[test]
fn render_markdown_flat() {
    let opts = ModelOptions {
        format: ModelFormat::Markdown,
        group_by: GroupBy::None,
        ..ModelOptions::default()
    };
    let output = exported(&multi_extension_schema(), opts);
    assert_snapshot!(output);
}

#[test]
fn render_markdown_empty() {
    let output = exported(
        &GraphProtocolSchema::empty(),
        default_options(ModelFormat::Markdown),
    );
    assert_snapshot!(output);
}

// --- Mermaid ---

#[test]
fn render_mermaid_keys_grouped() {
    let output = exported(
        &multi_extension_schema(),
        default_options(ModelFormat::Mermaid),
    );
    assert_snapshot!(output);
}

#[test]
fn render_mermaid_none_fields() {
    let output = exported(
        &multi_extension_schema(),
        ModelOptions {
            fields: FieldLevel::None,
            ..default_options(ModelFormat::Mermaid)
        },
    );
    assert_snapshot!(output);
}

#[test]
fn render_mermaid_empty() {
    let output = exported(
        &GraphProtocolSchema::empty(),
        default_options(ModelFormat::Mermaid),
    );
    assert_snapshot!(output);
}

// --- DOT ---

#[test]
fn render_dot_keys_grouped() {
    let output = export(
        &multi_extension_schema(),
        &builtin_colors(),
        &default_options(ModelFormat::Dot),
    );
    assert_snapshot!(output);
}

#[test]
fn render_dot_none_fields() {
    let output = export(
        &multi_extension_schema(),
        &builtin_colors(),
        &ModelOptions {
            fields: FieldLevel::None,
            ..default_options(ModelFormat::Dot)
        },
    );
    assert_snapshot!(output);
}

#[test]
fn render_dot_empty() {
    let output = exported(
        &GraphProtocolSchema::empty(),
        default_options(ModelFormat::Dot),
    );
    assert_snapshot!(output);
}

// --- JSON ---

#[specforge_test_macros::test(
    behavior = "render_model_json",
    verify = "output carries the schema_version specforge schema prints for the project"
)]
fn render_json_keys() {
    let output = exported(
        &multi_extension_schema(),
        default_options(ModelFormat::Json),
    );
    // Verify it's valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
    assert_eq!(parsed["schema_version"], "1.0.0");
    assert!(parsed["entities"].is_array());
    assert!(parsed["relationships"].is_array());
    assert_snapshot!(output);
}

#[test]
fn render_json_cardinality_strings() {
    let output = exported(
        &multi_extension_schema(),
        default_options(ModelFormat::Json),
    );
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
    for rel in parsed["relationships"].as_array().unwrap() {
        let card = rel["cardinality"].as_str().unwrap();
        assert!(
            ["1:1", "1:N", "N:1", "N:M"].contains(&card),
            "unexpected cardinality: {}",
            card
        );
    }
}

#[test]
fn render_json_empty() {
    let output = exported(
        &GraphProtocolSchema::empty(),
        default_options(ModelFormat::Json),
    );
    assert_snapshot!(output);
}

// --- DBML ---

#[test]
fn render_dbml_keys_grouped() {
    let output = exported(
        &multi_extension_schema(),
        default_options(ModelFormat::Dbml),
    );
    assert_snapshot!(output);
}

#[test]
fn render_dbml_empty() {
    let output = exported(
        &GraphProtocolSchema::empty(),
        default_options(ModelFormat::Dbml),
    );
    assert_snapshot!(output);
}

// C13-03: registry-declared dot_color wins over the extension palette.
#[test]
fn declared_dot_color_reaches_model_dot() {
    let mut schema = GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
        extensions: vec![SchemaExtensionInfo {
            name: "@specforge/software".to_string(),
            version: "1.0.0".to_string(),
        }],
        entity_kinds: vec![
            SchemaEntityKind {
                name: "behavior".to_string(),
                source_extension: "@specforge/software".to_string(),
                testable: true,
                dot_color: Some("#123456".to_string()),
                fields: vec![],
            },
            SchemaEntityKind {
                name: "event".to_string(),
                source_extension: "@specforge/software".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
        ],
        edge_types: vec![],
    };
    schema.extensions = vec![SchemaExtensionInfo {
        name: "@specforge/software".to_string(),
        version: "1.0.0".to_string(),
    }];

    // `All`: the unfiltered model, as the test always drew it.
    let output = export(
        &schema,
        &builtin_colors(),
        &ModelOptions {
            fields: FieldLevel::All,
            ..default_options(ModelFormat::Dot)
        },
    );

    assert!(
        output.contains("#123456"),
        "declared per-kind color emitted: {output}"
    );
    let software_palette = "#4a90d9";
    assert!(
        output.contains(software_palette),
        "undeclared kinds keep the extension's theme colour: {output}"
    );
}

// C13-10: DBML renders real column types and cardinality-matching Ref
// operators instead of all-string columns and hardcoded `>`.
#[test]
fn dbml_maps_real_types_and_cardinality_operators() {
    let schema = GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
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
                fields: vec![
                    SchemaField {
                        name: "count".to_string(),
                        field_type: FieldType::Integer,
                        required: false,
                        enum_values: None,
                        edge: None,
                        target_kind: None,
                        description: None,
                        default_value: None,
                        source_extension: "@specforge/software".to_string(),
                    },
                    SchemaField {
                        name: "flag".to_string(),
                        field_type: FieldType::Bool,
                        required: false,
                        enum_values: None,
                        edge: None,
                        target_kind: None,
                        description: None,
                        default_value: None,
                        source_extension: "@specforge/software".to_string(),
                    },
                    SchemaField {
                        name: "tags".to_string(),
                        field_type: FieldType::StringList,
                        required: false,
                        enum_values: None,
                        edge: None,
                        target_kind: None,
                        description: None,
                        default_value: None,
                        source_extension: "@specforge/software".to_string(),
                    },
                    SchemaField {
                        name: "body".to_string(),
                        field_type: FieldType::Block,
                        required: false,
                        enum_values: None,
                        edge: None,
                        target_kind: None,
                        description: None,
                        default_value: None,
                        source_extension: "@specforge/software".to_string(),
                    },
                    SchemaField {
                        name: "features".to_string(),
                        field_type: FieldType::ReferenceList,
                        required: false,
                        enum_values: None,
                        edge: Some("BehaviorImplementsFeature".to_string()),
                        target_kind: Some("feature".to_string()),
                        description: None,
                        default_value: None,
                        source_extension: "@specforge/software".to_string(),
                    },
                ],
            },
            SchemaEntityKind {
                name: "feature".to_string(),
                source_extension: "@specforge/software".to_string(),
                testable: false,
                dot_color: None,
                fields: vec![],
            },
        ],
        edge_types: vec![SchemaEdgeType {
            label: "BehaviorImplementsFeature".to_string(),
            source_extension: "@specforge/software".to_string(),
            source_kinds: Some(vec!["behavior".to_string()]),
            target_kinds: Some(vec!["feature".to_string()]),
        }],
    };

    let output = exported(
        &schema,
        ModelOptions {
            fields: FieldLevel::All,
            ..default_options(ModelFormat::Dbml)
        },
    );

    assert!(output.contains("count integer"), "integer mapped: {output}");
    assert!(output.contains("flag boolean"), "boolean mapped: {output}");
    assert!(output.contains("tags text"), "string_list mapped: {output}");
    assert!(output.contains("body json"), "block mapped: {output}");
    // reference_list -> ManyToMany -> `<>`, target column is the PK.
    assert!(
        output.contains("Ref BehaviorImplementsFeature: behavior.features <> feature.id"),
        "cardinality operator and PK column: {output}"
    );
}

#[test]
fn an_extension_without_a_theme_color_is_drawn_grey() {
    let output = exported(
        &multi_extension_schema(),
        ModelOptions {
            fields: FieldLevel::All,
            ..default_options(ModelFormat::Dot)
        },
    );
    assert!(output.contains("color=\"#95a5a6\";"), "{output}");
    assert!(!output.contains("#4a90d9"), "no palette by name: {output}");
}

// =========================================================================
// Declared text in the text diagrams (plan 16)
// =========================================================================

/// `multi_extension_schema()` with `behavior` renamed to `kind` and its
/// `contract` field's description replaced.
fn with_text(kind: &str, description: &str) -> GraphProtocolSchema {
    let mut schema = multi_extension_schema();
    for entity in &mut schema.entity_kinds {
        if entity.name == "behavior" {
            entity.name = kind.to_string();
            for field in &mut entity.fields {
                if field.name == "contract" {
                    field.description = Some(description.to_string());
                }
            }
        }
    }
    for edge in &mut schema.edge_types {
        if let Some(sources) = &mut edge.source_kinds {
            for source in sources.iter_mut().filter(|s| *s == "behavior") {
                *source = kind.to_string();
            }
        }
    }
    schema
}

fn rendered(schema: &GraphProtocolSchema, fields: FieldLevel, format: ModelFormat) -> String {
    exported(
        schema,
        ModelOptions {
            format,
            fields,
            ..ModelOptions::default()
        },
    )
}

#[specforge_test_macros::test(
    behavior = "render_model_mermaid",
    verify = "declared text with a quote, markup or a line break stays inside its Mermaid string"
)]
fn a_quote_or_line_break_in_a_description_stays_inside_the_er_string() {
    let schema = with_text("behavior", "the \"body\"\nnext");
    let output = rendered(&schema, FieldLevel::All, ModelFormat::Mermaid);
    assert!(output.contains("\"the #quot;body#quot; next\""), "{output}");
}

#[specforge_test_macros::test(
    behavior = "render_model_mermaid",
    verify = "an entity or attribute name that is not a Mermaid name is written as one"
)]
fn a_kind_name_that_is_not_an_identifier_is_made_one() {
    let schema = with_text("no\"te", "the contract");
    let output = rendered(&schema, FieldLevel::All, ModelFormat::Mermaid);
    assert!(output.contains("    no_te {"), "{output}");
    assert!(!output.contains("no\"te"), "{output}");
}

#[specforge_test_macros::test(
    behavior = "render_model_markdown",
    verify = "declared text with a pipe or a line break stays in its table cell"
)]
fn a_pipe_in_any_cell_stays_in_its_cell() {
    let mut schema = multi_extension_schema();
    for entity in &mut schema.entity_kinds {
        for field in &mut entity.fields {
            if field.name == "contract" {
                field.name = "con|tract".to_string();
                field.description = Some("a|b\nc".to_string());
            }
        }
    }
    let output = rendered(&schema, FieldLevel::All, ModelFormat::Markdown);
    let row = output
        .lines()
        .find(|line| line.starts_with("| con"))
        .expect("the field's row");
    assert!(row.starts_with("| con\\|tract |"), "{row}");
    assert!(row.ends_with("| a\\|b c |"), "{row}");
    assert_eq!(row.replace("\\|", "").matches('|').count(), 7, "{row}");
}

#[specforge_test_macros::test(
    behavior = "render_model_dbml",
    verify = "a name or note with a quote, an apostrophe or a line break stays one DBML name or string"
)]
fn an_apostrophe_or_line_break_stays_inside_a_dbml_note() {
    let schema = with_text("behavior", "this event's shape");
    let output = rendered(&schema, FieldLevel::All, ModelFormat::Dbml);
    assert!(output.contains("note: 'this event\\'s shape'"), "{output}");

    let schema = with_text("behavior", "first\nsecond");
    let output = rendered(&schema, FieldLevel::All, ModelFormat::Dbml);
    assert!(output.contains("note: 'first\\nsecond'"), "{output}");
}

#[specforge_test_macros::test(
    behavior = "render_model_dbml",
    verify = "each reference is one named Ref between columns the output writes"
)]
fn a_reference_is_one_named_ref() {
    let output = rendered(
        &multi_extension_schema(),
        FieldLevel::All,
        ModelFormat::Dbml,
    );
    assert!(!output.contains("ref:"), "{output}");
    assert_eq!(
        output
            .matches("Ref BehaviorImplementsFeature: behavior.features <> feature.id")
            .count(),
        1,
        "{output}"
    );
    assert!(
        output.contains("features text [note: 'BehaviorImplementsFeature -> feature']"),
        "{output}"
    );
}

#[specforge_test_macros::test(
    behavior = "render_model_dbml",
    verify = "each reference is one named Ref between columns the output writes"
)]
fn a_ref_joins_only_columns_the_output_writes() {
    let schema = multi_extension_schema();
    let output = rendered(&schema, FieldLevel::None, ModelFormat::Dbml);
    assert!(!output.contains("Ref "), "{output}");
    assert!(!output.contains("// ── Relationships ──"), "{output}");

    // `behavior` alone: its reference targets `feature`, which is not written.
    let options = ModelOptions {
        format: ModelFormat::Dbml,
        fields: FieldLevel::All,
        kinds: vec!["behavior".to_string()],
        ..ModelOptions::default()
    };
    let output = exported(&schema, options);
    assert!(output.contains("Table behavior {"), "{output}");
    assert!(!output.contains("Ref "), "{output}");
    assert!(!output.contains("Table feature"), "{output}");
}

#[specforge_test_macros::test(
    behavior = "render_model_dbml",
    verify = "a name or note with a quote, an apostrophe or a line break stays one DBML name or string"
)]
fn a_name_that_is_not_a_dbml_identifier_is_quoted() {
    let mut schema = multi_extension_schema();
    for info in &mut schema.extensions {
        if info.name == "@specforge/software" {
            info.name = "@acme/x".to_string();
        }
    }
    for entity in &mut schema.entity_kinds {
        if entity.source_extension == "@specforge/software" {
            entity.source_extension = "@acme/x".to_string();
        }
    }
    let output = rendered(&schema, FieldLevel::Keys, ModelFormat::Dbml);
    assert!(output.contains("TableGroup \"@acme/x\" {"), "{output}");

    let schema = with_text("no\"te", "the contract");
    let output = rendered(&schema, FieldLevel::All, ModelFormat::Dbml);
    assert!(output.contains("Table \"no\\\"te\" {"), "{output}");
}

#[specforge_test_macros::test(
    behavior = "render_model_dot",
    verify = "header row colored by extension"
)]
fn an_extensions_declared_theme_color_draws_its_cluster() {
    let schema = multi_extension_schema();
    let grey = exported(&schema, default_options(ModelFormat::Dot));
    let themed = export(
        &schema,
        &builtin_colors(),
        &default_options(ModelFormat::Dot),
    );
    assert!(!grey.contains("#4a90d9"), "{grey}");
    assert!(themed.contains("#4a90d9"), "software's colour: {themed}");
    assert!(themed.contains("#2ecc71"), "product's colour: {themed}");
}

#[specforge_test_macros::test(
    behavior = "filter_model",
    verify = "an empty kind list selects every kind"
)]
fn an_empty_kind_list_selects_every_kind() {
    let schema = multi_extension_schema();
    let all = json_of(&schema, ModelOptions::default());
    let empty = json_of(
        &schema,
        ModelOptions {
            kinds: vec![],
            ..ModelOptions::default()
        },
    );
    assert_eq!(names(&empty["entities"]), names(&all["entities"]));
    assert!(!names(&all["entities"]).is_empty());
}

#[specforge_test_macros::test(
    behavior = "build_model_intermediate",
    verify = "an extension's edge count is the edge types the model draws"
)]
fn an_extension_counts_the_edge_types_the_model_draws() {
    let kind = |name: &str| SchemaEntityKind {
        name: name.to_string(),
        source_extension: "@specforge/software".to_string(),
        testable: false,
        dot_color: None,
        fields: vec![],
    };
    let schema = GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
        extensions: vec![SchemaExtensionInfo {
            name: "@specforge/software".to_string(),
            version: "1.0.0".to_string(),
        }],
        entity_kinds: vec![kind("behavior"), kind("event")],
        edge_types: vec![
            SchemaEdgeType {
                label: "Triggers".to_string(),
                source_extension: "@specforge/software".to_string(),
                source_kinds: Some(vec!["behavior".to_string()]),
                target_kinds: Some(vec!["event".to_string()]),
            },
            SchemaEdgeType {
                label: "References".to_string(),
                source_extension: "@specforge/software".to_string(),
                source_kinds: None,
                target_kinds: None,
            },
        ],
    };

    let model = built(&schema);

    assert_eq!(model["relationships"].as_array().unwrap().len(), 1);
    // `References` is a standalone edge type: it draws nothing, so it is not counted.
    assert_eq!(model["extensions"][0]["edge_count"], 1);
}

#[test]
fn an_enhancements_edge_types_count_for_their_owner() {
    let kind = |name: &str, extension: &str| SchemaEntityKind {
        name: name.to_string(),
        source_extension: extension.to_string(),
        testable: false,
        dot_color: None,
        fields: vec![],
    };
    let extension = |name: &str| SchemaExtensionInfo {
        name: name.to_string(),
        version: "1.0.0".to_string(),
    };
    let schema = GraphProtocolSchema {
        schema_version: SchemaVersion::new(1, 0, 0),
        extensions: vec![extension("@x/soft"), extension("@x/formal")],
        entity_kinds: vec![kind("behavior", "@x/soft"), kind("invariant", "@x/soft")],
        edge_types: vec![SchemaEdgeType {
            label: "Proves".to_string(),
            source_extension: "@x/formal".to_string(),
            source_kinds: Some(vec!["behavior".to_string()]),
            target_kinds: Some(vec!["invariant".to_string()]),
        }],
    };

    let model = json_of(
        &schema,
        ModelOptions {
            kinds: vec!["behavior".to_string(), "invariant".to_string()],
            ..ModelOptions::default()
        },
    );

    let counts: Vec<(String, u64, u64)> = model["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["name"].as_str().unwrap().to_string(),
                e["entity_count"].as_u64().unwrap(),
                e["edge_count"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        counts,
        [
            ("@x/soft".to_string(), 2, 0),
            ("@x/formal".to_string(), 0, 1)
        ]
    );
}
