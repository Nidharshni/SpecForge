use serde_json::Value;
use specforge_emitter::model::{FieldLevel, ModelFormat, ModelOptions, ModelRoot};
use specforge_ops::OpErrorKind;
use specforge_ops::model::{ModelOutcome, model};
use specforge_protocol_types::{
    EdgeTypeDescriptor, EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor,
    HandshakeResponse,
};
use specforge_registry::build_registries;
use specforge_test::prelude::*;

use crate::view_support::Project;

#[specforge_test(
    behavior = "build_model_intermediate",
    verify = "a field's type is named as its extension declares it; DBML writes its own column type"
)]
fn the_model_names_a_field_type_as_declared() {
    // As `@specforge/formal` declares `abstract`, through the registry build
    // (the model groups kinds by the extensions that declared them).
    let mut declaration = specforge_protocol_types::ExtensionDeclaration::default();
    declaration.handshake.name = "@t/formal".to_string();
    declaration.entities = vec![specforge_protocol_types::EntityKindDescriptor {
        name: "behavior".to_string(),
        keyword: Some("behavior".to_string()),
        fields: vec![specforge_protocol_types::FieldDescriptor {
            name: "abstract".to_string(),
            field_type: "bool".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    }];
    let project = Project::new("", specforge_registry::build_registries(vec![declaration]));
    let rendered = |format| {
        let options = ModelOptions {
            format,
            fields: FieldLevel::All,
            ..ModelOptions::default()
        };
        model(&project.view(), &options)
            .expect("no refusal")
            .document
    };

    let markdown = rendered(ModelFormat::Markdown);
    assert!(markdown.contains("| abstract | bool |"), "{markdown}");
    assert!(rendered(ModelFormat::Mermaid).contains("bool abstract"));
    assert!(rendered(ModelFormat::Dot).contains("abstract</td><td>bool</td>"));
    // DBML writes its own column type.
    assert!(rendered(ModelFormat::Dbml).contains("abstract boolean"));
    let json: serde_json::Value =
        serde_json::from_str(&rendered(ModelFormat::Json)).expect("the json model parses");
    let abstract_field = json["entities"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|e| e["fields"].as_array().into_iter().flatten())
        .find(|f| f["name"] == "abstract")
        .expect("abstract is a field of the model");
    assert_eq!(abstract_field["field_type"], "bool");
}

/// `@t/soft` declares `behavior` (its `events` a reference list to `event`
/// over `Triggers`) and `event`; `@t/test` is loaded and declares no kind.
fn two_extensions() -> Project {
    let declaration = |name: &str| ExtensionDeclaration {
        handshake: HandshakeResponse {
            name: name.into(),
            version: "1.0.0".into(),
            ..HandshakeResponse::default()
        },
        ..ExtensionDeclaration::default()
    };
    let mut soft = declaration("@t/soft");
    soft.entities = vec![
        EntityKindDescriptor {
            name: "behavior".into(),
            fields: vec![FieldDescriptor {
                name: "events".into(),
                field_type: "reference_list".into(),
                edge: Some("Triggers".into()),
                target_kind: Some("event".into()),
                ..FieldDescriptor::default()
            }],
            ..EntityKindDescriptor::default()
        },
        EntityKindDescriptor {
            name: "event".into(),
            ..EntityKindDescriptor::default()
        },
    ];
    soft.edges = vec![EdgeTypeDescriptor {
        label: "Triggers".into(),
        source_kind: Some("behavior".into()),
        target_kind: Some("event".into()),
        ..EdgeTypeDescriptor::default()
    }];
    Project::new("", build_registries(vec![soft, declaration("@t/test")]))
}

/// The model operation over `project`, drawn as JSON.
fn outcome_of(
    project: &Project,
    options: ModelOptions,
) -> Result<ModelOutcome, specforge_ops::Refusal<specforge_ops::model::ModelField>> {
    model(
        &project.view(),
        &ModelOptions {
            format: ModelFormat::Json,
            ..options
        },
    )
}

/// The JSON model `options` draws over `project`.
fn json_model(project: &Project, options: ModelOptions) -> Value {
    let outcome = outcome_of(project, options).expect("no refusal");
    serde_json::from_str(&outcome.document).expect("the json model parses")
}

/// The kinds the JSON model lists, in order.
fn kinds_of(project: &Project, options: ModelOptions) -> Vec<String> {
    json_model(project, options)["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap().to_string())
        .collect()
}

/// Each extension of the JSON model: (name, entity_count, edge_count).
fn counts_of(project: &Project, options: ModelOptions) -> Vec<(String, u64, u64)> {
    json_model(project, options)["extensions"]
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
        .collect()
}

#[specforge_test(
    behavior = "filter_model",
    verify = "a root no loaded extension declares is refused with unknown_kind naming the closest declared kind"
)]
fn a_root_the_project_does_not_declare_is_refused() {
    let project = two_extensions();
    let root = |kind: &str| ModelOptions {
        root: Some(ModelRoot {
            kind: kind.into(),
            depth: None,
        }),
        ..ModelOptions::default()
    };
    let error = outcome_of(&project, root("behaviour")).unwrap_err();
    assert_eq!(error.error.kind, OpErrorKind::InvalidInput);
    assert_eq!(error.error.code, "unknown_kind");
    assert_eq!(error.error.message, "unknown entity kind 'behaviour'");
    assert_eq!(
        error.error.suggestion.as_deref(),
        Some("did you mean 'behavior'?")
    );
    let error = outcome_of(&project, root("zzzzzz")).unwrap_err();
    assert_eq!(error.error.code, "unknown_kind");
    assert_eq!(error.error.suggestion, None);
}

#[specforge_test(
    behavior = "filter_model",
    verify = "an extension the project does not load is refused, naming the loaded one meant"
)]
fn an_extension_the_project_does_not_load_is_refused() {
    let project = two_extensions();
    let extension = |name: &str| ModelOptions {
        extension: Some(name.into()),
        ..ModelOptions::default()
    };
    for (name, suggestion) in [
        ("soft", "did you mean '@t/soft'?"),
        ("@t/sotf", "did you mean '@t/soft'?"),
        ("@acme/none", "the project loads @t/soft, @t/test"),
    ] {
        let error = outcome_of(&project, extension(name)).unwrap_err();
        assert_eq!(error.error.kind, OpErrorKind::ExtensionNotFound, "{name}");
        assert_eq!(error.error.code, "extension_not_found", "{name}");
        assert_eq!(
            error.error.message,
            format!("extension '{name}' is not loaded by this project")
        );
        assert_eq!(
            error.error.suggestion.as_deref(),
            Some(suggestion),
            "{name}"
        );
    }
    // Loaded, and declares no kind: an empty model, not a refusal.
    assert!(kinds_of(&project, extension("@t/test")).is_empty());
}

#[specforge_test(
    behavior = "filter_model",
    verify = "a listed kind the project does not know is reported as I020 and selects nothing"
)]
fn a_listed_kind_the_project_does_not_know_is_reported() {
    let project = two_extensions();
    let outcome = outcome_of(
        &project,
        ModelOptions {
            kinds: vec!["behaviour".into(), "event".into()],
            ..ModelOptions::default()
        },
    )
    .expect("a kind filter does not refuse");
    let json: Value = serde_json::from_str(&outcome.document).unwrap();
    assert_eq!(json["entities"].as_array().unwrap().len(), 1);
    assert_eq!(json["entities"][0]["name"], "event");
    assert_eq!(outcome.notices.len(), 1);
    assert_eq!(outcome.notices[0].code, "I020");
    assert_eq!(
        outcome.notices[0].message,
        "unknown entity kind 'behaviour'"
    );
    assert_eq!(
        outcome.notices[0].suggestion.as_deref(),
        Some("did you mean 'behavior'?")
    );
}

#[specforge_test(
    behavior = "filter_model",
    verify = "Filter Model by Extension, Kind, or Depth: model filtering holds — model_ir_built, extension_filter_applied, kind_filter_applied, depth_filter_applied, edges_pruned, filters_compose, unknown_names_answered"
)]
fn model_filtering_contract() {
    let project = two_extensions();
    // model_ir_built: both kinds, one relationship.
    assert_eq!(
        kinds_of(&project, ModelOptions::default()),
        ["behavior", "event"]
    );
    // extension_filter_applied.
    assert_eq!(
        kinds_of(
            &project,
            ModelOptions {
                extension: Some("@t/soft".into()),
                ..ModelOptions::default()
            }
        ),
        ["behavior", "event"]
    );
    // kind_filter_applied.
    let kinds = ModelOptions {
        kinds: vec!["event".into()],
        ..ModelOptions::default()
    };
    assert_eq!(kinds_of(&project, kinds), ["event"]);
    // depth_filter_applied.
    let root = ModelOptions {
        root: Some(ModelRoot {
            kind: "behavior".into(),
            depth: Some(0),
        }),
        ..ModelOptions::default()
    };
    assert_eq!(kinds_of(&project, root), ["behavior"]);
    // edges_pruned: no relationship when `event` is filtered out.
    let pruned = json_model(
        &project,
        ModelOptions {
            kinds: vec!["behavior".into()],
            ..ModelOptions::default()
        },
    );
    assert!(pruned["relationships"].as_array().unwrap().is_empty());
    // filters_compose: an intersection.
    assert_eq!(
        kinds_of(
            &project,
            ModelOptions {
                extension: Some("@t/soft".into()),
                kinds: vec!["event".into()],
                ..ModelOptions::default()
            }
        ),
        ["event"]
    );
    // unknown_names_answered: a root and an extension are refused, a kind
    // is reported.
    let unknown = |options| outcome_of(&project, options);
    assert!(
        unknown(ModelOptions {
            root: Some(ModelRoot {
                kind: "behaviour".into(),
                depth: None
            }),
            ..ModelOptions::default()
        })
        .is_err()
    );
    assert!(
        unknown(ModelOptions {
            extension: Some("soft".into()),
            ..ModelOptions::default()
        })
        .is_err()
    );
    assert_eq!(
        unknown(ModelOptions {
            kinds: vec!["behaviour".into()],
            ..ModelOptions::default()
        })
        .unwrap()
        .notices
        .len(),
        1
    );
}

// P2 - kept.
#[specforge_test(
    behavior = "filter_model",
    verify = "multiple filters compose as intersection"
)]
fn the_model_filters_compose_as_an_intersection() {
    let project = two_extensions();
    // The root's kind is not kept when `kinds` omits it.
    assert_eq!(
        kinds_of(
            &project,
            ModelOptions {
                root: Some(ModelRoot {
                    kind: "event".into(),
                    depth: None,
                }),
                kinds: vec!["behavior".into()],
                ..ModelOptions::default()
            }
        ),
        ["behavior"]
    );
    assert_eq!(
        kinds_of(
            &project,
            ModelOptions {
                extension: Some("@t/soft".into()),
                kinds: vec!["event".into()],
                ..ModelOptions::default()
            }
        ),
        ["event"]
    );
    // Loaded, and declares no kind.
    assert!(
        kinds_of(
            &project,
            ModelOptions {
                extension: Some("@t/test".into()),
                ..ModelOptions::default()
            }
        )
        .is_empty()
    );
}

#[specforge_test(
    behavior = "filter_model",
    verify = "a selection recounts each extension's entities and drawn edge types"
)]
fn an_extensions_counts_after_a_filter_are_what_the_selection_draws() {
    let project = two_extensions();
    // `Triggers` joins behavior to event, which this selection drops: nothing is drawn.
    assert_eq!(
        counts_of(
            &project,
            ModelOptions {
                kinds: vec!["behavior".into()],
                ..ModelOptions::default()
            }
        ),
        [("@t/soft".into(), 1, 0), ("@t/test".into(), 0, 0)]
    );
    assert_eq!(
        counts_of(
            &project,
            ModelOptions {
                kinds: vec!["behavior".into(), "event".into()],
                ..ModelOptions::default()
            }
        ),
        [("@t/soft".into(), 2, 1), ("@t/test".into(), 0, 0)]
    );
    assert_eq!(
        counts_of(
            &project,
            ModelOptions {
                extension: Some("@t/test".into()),
                ..ModelOptions::default()
            }
        ),
        [("@t/soft".into(), 0, 0), ("@t/test".into(), 0, 0)]
    );
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn model_states_which_field_a_refusal_is_about() {
    use specforge_ops::model::ModelField;
    let project = two_extensions();
    let root = ModelOptions {
        root: Some(ModelRoot {
            kind: "zzzzzz".into(),
            depth: None,
        }),
        ..ModelOptions::default()
    };
    assert_eq!(
        outcome_of(&project, root).unwrap_err().subject,
        Some(ModelField::Root)
    );
    let extension = ModelOptions {
        extension: Some("@acme/none".into()),
        ..ModelOptions::default()
    };
    assert_eq!(
        outcome_of(&project, extension).unwrap_err().subject,
        Some(ModelField::Extension)
    );
}
